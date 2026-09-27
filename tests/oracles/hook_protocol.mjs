import { readFileSync } from "node:fs";
import { isDeepStrictEqual } from "node:util";

const [vectorPath, mode] = process.argv.slice(2);
if (!vectorPath) {
  throw new Error("usage: node hook_protocol.mjs <vectors.json> [--report]");
}

const fixtures = JSON.parse(readFileSync(vectorPath, "utf8"));
const sessions = new Map(
  fixtures.session_specs.map((fixture) => [fixture.name, fixture.raw_json]),
);

function u32be(value) {
  const output = Buffer.alloc(4);
  output.writeUInt32BE(value);
  return output;
}

function frame(rawJson) {
  const payload = Buffer.from(rawJson, "utf8");
  return Buffer.concat([u32be(payload.length), payload]);
}

function compareText(left, right) {
  return left < right ? -1 : left > right ? 1 : 0;
}

function processValid(vector) {
  const transcript = [
    {
      direction: "pactrun_to_hook",
      raw_json: sessions.get(vector.session),
    },
    ...vector.messages,
  ];
  if (!transcript[0].raw_json) {
    throw new Error(`unknown Session fixture ${vector.session}`);
  }

  let session;
  let riskState = "clear";
  let pendingRequest;
  let completion;
  for (const wire of transcript) {
    const message = JSON.parse(wire.raw_json);
    switch (message.type) {
      case "session_start":
        if (wire.direction !== "pactrun_to_hook" || message.protocol_version !== "1.0-alpha.1") {
          throw new Error(`${vector.name}: invalid session_start`);
        }
        session = message;
        break;
      case "session_ready":
        if (
          wire.direction !== "hook_to_pactrun" ||
          message.protocol_version !== "1.0-alpha.1" ||
          message.session_id !== session.session_id
        ) {
          throw new Error(`${vector.name}: invalid session_ready`);
        }
        break;
      case "diagnostic":
        if (wire.direction !== "hook_to_pactrun") {
          throw new Error(`${vector.name}: diagnostic direction`);
        }
        break;
      case "request":
        if (pendingRequest) {
          throw new Error(`${vector.name}: more than one outstanding request`);
        }
        pendingRequest = {
          id: message.request_id,
          kind: message.request.kind,
        };
        break;
      case "request_ack": {
        if (!pendingRequest || pendingRequest.id !== message.request_id) {
          throw new Error(`${vector.name}: request acknowledgment mismatch`);
        }
        const expected =
          pendingRequest.kind === "enter_recovery_risk" ? "open" : "clear";
        if (message.risk_state !== expected) {
          throw new Error(`${vector.name}: recovery state mismatch`);
        }
        riskState = expected;
        pendingRequest = undefined;
        break;
      }
      case "cancel":
      case "cancel_ack":
        break;
      case "complete": {
        completion = {
          completion: "submitted",
          operation: message.operation,
          risk_state: riskState,
        };
        if (message.operation === "action") {
          completion.submitted_outputs = [...message.produced_outputs].sort();
        } else if (message.operation === "migration") {
          completion.submitted_outputs = [...message.produced_target_outputs].sort();
        } else if (message.operation === "snapshot_capture") {
          completion.service_content = [...message.service_content]
            .map(({ role, path, candidate_path }) => ({ role, path, candidate_path }))
            .sort((left, right) =>
              compareText(left.role, right.role) || compareText(left.path, right.path),
            );
        }
        break;
      }
      case "completion_accepted":
        if (!completion) {
          throw new Error(`${vector.name}: completion was not submitted`);
        }
        completion.completion = "accepted";
        break;
      case "protocol_error":
        if (wire.direction !== "hook_to_pactrun") {
          throw new Error(`${vector.name}: protocol_error direction`);
        }
        completion = {
          completion: "protocol_error",
          operation: session.operation.kind,
          risk_state: riskState,
        };
        break;
      default:
        throw new Error(`${vector.name}: unsupported valid-fixture message ${message.type}`);
    }
  }
  if (!isDeepStrictEqual(completion, vector.expected_state)) {
    throw new Error(`${vector.name}: normalized valid-fixture state mismatch`);
  }
  return {
    name: vector.name,
    frames_hex: transcript.map((wire) => frame(wire.raw_json).toString("hex")),
    final_state: completion,
  };
}

// Test-ID: PR-TEST-0031
// Verifies: PR-REQ-0082, PR-REQ-0204, PR-REQ-0205, PR-REQ-0213, PR-REQ-0218
const report = {
  preamble_hex: Buffer.concat([
    Buffer.from("pactrun.hook-protocol\0", "ascii"),
    Buffer.from([0, 11]), Buffer.from("1.0-alpha.1", "ascii"),
  ]).toString("hex"),
  vectors: fixtures.valid.map(processValid),
};

if (mode === "--report") {
  process.stdout.write(JSON.stringify(report));
} else {
  process.stderr.write(
    `Hook protocol baseline Node 24 oracle: ${report.vectors.length} valid fixtures passed framing and normalized-state parity\n`,
  );
}
