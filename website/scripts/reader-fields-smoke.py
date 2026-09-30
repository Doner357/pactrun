"""Exercise reader field examples and refusals in a fresh, explicit test store."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True)
parser.add_argument('--workspace', required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parents[2]
binary = Path(args.binary).resolve(strict=True)
base = Path(args.workspace).resolve()
base.mkdir(parents=True, exist_ok=True)
work = Path(tempfile.mkdtemp(prefix='reader-fields-', dir=base))
for child in ('database', 'runtime-content', 'staging'):
    (work / 'store' / child).mkdir(parents=True, exist_ok=True)
env = dict(os.environ, PACTRUN_STORAGE_ROOT=str(work / 'store'))
commands = []

def run(*arguments, success=True, stdin=None):
    result = subprocess.run([str(binary), *arguments], cwd=work, env=env,
                            input=stdin, capture_output=True, text=True, encoding='utf-8', timeout=45)
    assert (result.returncode == 0) == success, (arguments, result.stdout, result.stderr)
    commands.append({'args': list(arguments), 'exit': result.returncode})
    return result.stdout

def blocks(name):
    return re.findall(r'^```yaml\n(.*?)^```', (root / name).read_text(encoding='utf-8'), re.M | re.S)

fields_name = 'docs/package-authors/reference/pack-fields.md'
service_name = 'docs/package-authors/reference/service-fields.md'
fields = blocks(fields_name)
input_block = next(block for block in fields if '\ninputs:' in block)
parameter_block = next(block for block in fields if '\nparameters:' in block)
metadata_block = next(block for block in fields if block.startswith('portable_metadata:'))
service_block = blocks(service_name)[0]
author = (root / 'docs/package-authors/fundamentals/authoring-model.md').read_text(encoding='utf-8')
if os.name == 'nt':
    author = author.split('### Windows variant\n')[1]
template = re.search(r'^```yaml\n(.*?)^```', author, re.M | re.S).group(1)
script_name = 'inspect.ps1' if os.name == 'nt' else 'inspect.sh'
script = ("& $env:PACTRUN_EXECUTABLE hook parameter message\nif ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }\n"
          if os.name == 'nt' else '"$PACTRUN_EXECUTABLE" hook parameter message\n')

def content(block):
    return '\n'.join(line for line in block.splitlines() if not line.startswith('#')).strip() + '\n'

def manifest(revision='{}', metadata=''):
    package = run('pack', 'generate-id').strip()
    value = '{}\n' if revision == '{}' else '\n' + ''.join('  ' + line + '\n' for line in content(revision).splitlines())
    return 'source_format: "1.0-alpha.1"\npackage_id: "' + package + '"\nrevision: ' + value + 'runtime_content: {}\n' + metadata

def install(name, source, success=True, hook=False):
    directory = work / name
    directory.mkdir()
    (directory / 'pactrun.yaml').write_text(source, encoding='utf-8')
    if hook:
        (directory / script_name).write_text(script, encoding='utf-8')
    result = run('pack', 'install', str(directory), success=success)
    return re.search(r'^exact:[a-f0-9]{32}/sha256:[a-f0-9]{64}$', result, re.M).group() if success else None

required_source = manifest(input_block)
reference = install('input', required_source)
run('instance', 'create', 'reader-input', '--revision', reference)
assert 'required_inputs_satisfied: false' in run('instance', 'show', 'reader-input')
run('instance', 'delete', 'reader-input')
install('quoted-boolean', required_source.replace('required: true', 'required: "true"'), success=False)

parameter_lines = content(parameter_block).splitlines()
parameter_lines.extend([
    '  - {id: count, type: integer, sensitive: false, default: 0}',
    '  - {id: ratio, type: float, sensitive: false, default: 0.0}',
    '  - {id: enabled, type: boolean, sensitive: false, default: false}',
])
parameter_source = template.replace('      parameters: []', '\n'.join('      ' + line for line in parameter_lines))
parameter_source = re.sub(r'package_id: "[a-f0-9]{32}"', 'package_id: "' + run('pack', 'generate-id').strip() + '"', parameter_source)
reference = install('parameter', parameter_source, hook=True)
run('instance', 'create', 'reader-parameter', '--revision', reference)
run('invoke', 'reader-parameter', 'inspect', '--plan')
assert '"hello"' in run('invoke', 'reader-parameter', 'inspect')
lexical_cases = [
    ('count', '0', True), ('count', '-0', True), ('count', '9007199254740991', True),
    ('count', '-9007199254740991', True), ('count', '+1', False), ('count', '01', False),
    ('count', '1.0', False), ('count', '1e0', False), ('count', '9007199254740992', False),
    ('count', ' 1', False), ('ratio', '0', True), ('ratio', '1.5', True),
    ('ratio', '1E+3', True), ('ratio', '1e-400', True), ('ratio', '.5', False),
    ('ratio', '1.', False), ('ratio', 'NaN', False), ('ratio', 'Infinity', False),
    ('ratio', '1e400', False), ('ratio', '1\n', False), ('enabled', 'true', True),
    ('enabled', 'false', True), ('enabled', 'True', False), ('enabled', '0', False),
    ('enabled', 'false\n', False), ('message', '', True), ('message', '  value  ', True),
]
for parameter, value, accepted in lexical_cases:
    run('invoke', 'reader-parameter', 'inspect', '--param', parameter + '=' + value, '--plan', success=accepted)
file_cases = [('count', '1', True), ('count', '1\n', False), ('count', '\ufeff1', False),
              ('ratio', '1.5', True), ('ratio', '1.5\n', False),
              ('enabled', 'false\n', False), ('message', '', True), ('message', '\ufefftext\n', True)]
for index, (parameter, value, accepted) in enumerate(file_cases):
    value_file = work / ('parameter-' + str(index) + '.txt')
    value_file.write_bytes(value.encode('utf-8'))
    run('invoke', 'reader-parameter', 'inspect', '--param-file', parameter + '=' + str(value_file), '--plan', success=accepted)
run('invoke', 'reader-parameter', 'inspect', '--param-stdin', 'enabled', '--plan', stdin='false')
run('invoke', 'reader-parameter', 'inspect', '--param-stdin', 'enabled', '--plan', stdin='false\n', success=False)
run('instance', 'delete', 'reader-parameter')
install('bad-type', parameter_source.replace('type: string', 'type: enum'), success=False, hook=True)
install('bad-terminal', parameter_source.replace('terminal: output', 'terminal: quiet'), success=False, hook=True)
install('numeric-protocol', parameter_source.replace('protocol_version: "1.0-alpha.1"', 'protocol_version: 1'), success=False, hook=True)
reference = install('unsupported-protocol', parameter_source.replace('protocol_version: "1.0-alpha.1"', 'protocol_version: "1.0-alpha.2"'), hook=True)
run('instance', 'create', 'reader-unsupported', '--revision', reference)
run('invoke', 'reader-unsupported', 'inspect', '--plan', success=False)
run('instance', 'delete', 'reader-unsupported')

reference = install('service', manifest(service_block))
run('instance', 'create', 'reader-service', '--revision', reference)
run('resource', 'observe', 'reader-service', 'database')
run('resource', 'locate', 'reader-service', 'database', '--intent', 'read', success=False)
run('instance', 'delete', 'reader-service')

reference = install('metadata', manifest(metadata=metadata_block))
assert 'Example service' in run('revision', 'metadata', 'show', reference)
receipt = {'status': 'Passed', 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
           'documents': {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in [fields_name, service_name, 'docs/pactrun-users/operations/invoke-reference.md']},
           'commands': commands, 'parameter_text_cases': lexical_cases, 'parameter_file_cases': file_cases,
           'retained_workspace': str(work)}
(work / 'receipt.json').write_text(json.dumps(receipt, indent=2), encoding='utf-8')
print(json.dumps(receipt, indent=2))
