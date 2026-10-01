"""Linux trial pack. Only public Pactrun Shell Loader helpers are used."""
import hashlib
import ipaddress
import json
import os
from pathlib import Path
import re
import subprocess
import sys

PHASE = "session"


def helper(*args):
    result = subprocess.run([os.environ["PACTRUN_EXECUTABLE"], "hook", *args],
                            capture_output=True, text=True, check=True)
    return result.stdout.rstrip("\r\n")


def docker(*args, variables=None):
    env = {k: v for k, v in os.environ.items() if not k.startswith("COMPOSE_")}
    env.update(variables or {})
    result = subprocess.run(["docker", *args], env=env, capture_output=True,
                            text=True, check=True)
    return result.stdout


def safe_child(parent, name):
    path = parent / name
    if path.is_symlink():
        raise ValueError("linked path")
    return path


def read_input(name):
    return json.loads(Path(helper("input", name)).read_text(encoding="utf-8"))


def validate(config, credentials):
    if set(config) != {"authentik_image", "postgres_image", "redis_image",
                      "bind_address", "http_port", "https_port"}:
        raise ValueError("config fields")
    for key, repo in (("authentik_image", "ghcr.io/goauthentik/server"),
                      ("postgres_image", "postgres"), ("redis_image", "redis")):
        if not isinstance(config[key], str) or not re.fullmatch(
                re.escape(repo) + r"@sha256:[0-9a-f]{64}", config[key]):
            raise ValueError("immutable image digest required")
    ipaddress.IPv4Address(config["bind_address"])
    for key in ("http_port", "https_port"):
        if type(config[key]) is not int or not 1024 <= config[key] <= 65535:
            raise ValueError("port range/type")
    if config["http_port"] == config["https_port"]:
        raise ValueError("duplicate port")
    if set(credentials) != {"postgres_password", "secret_key", "bootstrap_password", "bootstrap_token"}:
        raise ValueError("credential fields")
    if any(not isinstance(v, str) or not re.fullmatch(r"[0-9a-f]{64}", v)
           for v in credentials.values()):
        raise ValueError("64 hexadecimal characters per secret required")


def local_engine():
    if os.environ.get("DOCKER_HOST") or os.environ.get("DOCKER_CONTEXT"):
        raise ValueError("ambient remote overrides refused")
    context = json.loads(docker("context", "inspect"))[0]
    if not context["Endpoints"]["docker"]["Host"].startswith("unix://"):
        raise ValueError("local Linux engine required")
    docker("version", "--format", "{{.Server.Version}}")
    docker("compose", "version", "--short")


def compose_document(config, deployment, project):
    if os.getuid() == 0:
        raise ValueError('evaluation requires an unprivileged Pactrun user')
    database_user = f'{os.getuid()}:{os.getgid()}'
    def mount(name, target):
        return {"type": "bind", "source": str(safe_child(deployment, name)), "target": target}

    env = {"AUTHENTIK_POSTGRESQL__HOST": "postgresql",
           "AUTHENTIK_POSTGRESQL__NAME": "authentik", "AUTHENTIK_POSTGRESQL__USER": "authentik",
           "AUTHENTIK_POSTGRESQL__PASSWORD": "${PACK_POSTGRES_PASSWORD:?required}",
           "AUTHENTIK_SECRET_KEY": "${PACK_SECRET_KEY:?required}",
           "AUTHENTIK_REDIS__HOST": "redis", "AUTHENTIK_ERROR_REPORTING__ENABLED": "false"}
    common = {"image": config["authentik_image"], "restart": "unless-stopped",
              "depends_on": {"postgresql": {"condition": "service_healthy"},
                             "redis": {"condition": "service_healthy"}},
              "volumes": [mount("media", "/media"), mount("certs", "/certs"),
                          mount("templates", "/templates")]}
    worker_env = dict(env, AUTHENTIK_BOOTSTRAP_EMAIL="blackbox@example.invalid",
                      AUTHENTIK_BOOTSTRAP_PASSWORD="${PACK_BOOTSTRAP_PASSWORD:?required}",
                      AUTHENTIK_BOOTSTRAP_TOKEN="${PACK_BOOTSTRAP_TOKEN:?required}")
    return {"name": project, "services": {
        "postgresql": {"image": config["postgres_image"], "user": database_user, "restart": "unless-stopped",
                       "environment": {"POSTGRES_DB": "authentik", "POSTGRES_USER": "authentik",
                                       "POSTGRES_PASSWORD": "${PACK_POSTGRES_PASSWORD:?required}"},
                       "volumes": [mount("postgres", "/var/lib/postgresql/data")],
                       "healthcheck": {"test": ["CMD-SHELL", "pg_isready -U authentik -d authentik"],
                                       "interval": "5s", "timeout": "5s", "retries": 60}},
        "redis": {"image": config["redis_image"], "user": database_user, "restart": "unless-stopped",
                  "command": ["--save", "60", "1", "--loglevel", "warning"],
                  "volumes": [mount("redis", "/data")],
                  "healthcheck": {"test": ["CMD", "redis-cli", "ping"],
                                  "interval": "5s", "timeout": "5s", "retries": 60}},
        "server": dict(common, command="server", environment=env,
                       ports=[f'{config["bind_address"]}:{config["http_port"]}:9000',
                              f'{config["bind_address"]}:{config["https_port"]}:9443']),
        "worker": dict(common, command="worker", user="0:0", environment=worker_env)}}


def main(operation):
    global PHASE
    session = json.loads(helper("session"))
    if operation != "check":
        authorities = [a for a in session["service_authorities"]
                       if a["reference"]["kind"] == "storage" and a["reference"]["id"] == "state"]
        if len(authorities) != 1:
            raise ValueError("storage authority")
        state = Path(helper("resource", authorities[0]["handle"]))
        deployment = safe_child(state, "deployment")
        contract = safe_child(deployment, "compose.json")
        if operation != "start" and not contract.exists():
            print("No deployment has been prepared.")
            return
    if operation in ("check", "start"):
        PHASE = "input-validation"
        config, credentials = read_input("config"), read_input("credentials")
        validate(config, credentials)
    PHASE = "local-engine-preflight"
    local_engine()
    if operation == "check":
        print("Inputs, local engine and Compose available; application health not asserted.")
        return
    PHASE = "deployment-contract"
    if operation == "start":
        project = "pactrun-ak-" + hashlib.sha256(str(state).encode()).hexdigest()[:24]
        doc = compose_document(config, deployment, project)
        credential_hash = hashlib.sha256(json.dumps(credentials, sort_keys=True).encode()).hexdigest()
        lock = safe_child(deployment, "credential-hash.txt")
        if contract.exists():
            if json.loads(contract.read_text()) != doc or lock.read_text() != credential_hash:
                raise ValueError("implicit configuration change/rotation refused")
        else:
            deployment.mkdir(mode=0o700)
            for name in ("postgres", "redis", "media", "certs", "templates"):
                safe_child(deployment, name).mkdir(mode=0o755)
            lock.write_text(credential_hash)
            contract.write_text(json.dumps(doc, indent=2, sort_keys=True))
        variables = {"PACK_" + k.upper(): v for k, v in credentials.items()}
    else:
        doc = json.loads(contract.read_text())
        project = doc["name"]
        if not re.fullmatch(r"pactrun-ak-[0-9a-f]{24}", project):
            raise ValueError("saved project identity")
        variables = {"PACK_" + k: "unused-for-lifecycle" for k in
                     ("POSTGRES_PASSWORD", "SECRET_KEY", "BOOTSTRAP_PASSWORD", "BOOTSTRAP_TOKEN")}
    command = ("compose", "--project-name", project, "--project-directory", str(deployment),
               "--env-file", str(Path(__file__).with_name("empty.env")), "-f", str(contract))
    PHASE = "compose-" + operation
    if operation == "start":
        helper("risk", "enter")
        docker(*command, "up", "--detach", "--wait", "--wait-timeout", "300", variables=variables)
        helper("risk", "resolve")
        print("Compose start completed; verify application separately.")
    elif operation == "stop":
        docker(*command, "stop", "--timeout", "30", variables=variables)
        print("Containers stopped; files retained.")
    elif operation == "status":
        print(docker(*command, "ps", "--all", "--format", "{{.Service}} {{.State}} {{.Health}}", variables=variables))
    elif operation == "cleanup":
        docker(*command, "down", "--timeout", "30", variables=variables)
        print("Compose project removed; this Hook did not delete data directories.")
    else:
        raise ValueError("unknown operation")


if __name__ == "__main__":
    try:
        main(sys.argv[1])
    except Exception as error:
        code = error.returncode if isinstance(error, subprocess.CalledProcessError) else None
        print(f"Pack failed at {PHASE}; category={type(error).__name__}; exit={code}. Sensitive details withheld.", file=sys.stderr)
        sys.exit(1)
