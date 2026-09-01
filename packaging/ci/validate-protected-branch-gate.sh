#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)
authority="$script_dir/protected-branch-authority.py"
policy="$repository_root/docs/contracts/v1/protected-branch-policy-v1.json"
run_id=$(date -u +%Y%m%dT%H%M%SZ)-$$
log_root="$repository_root/target/t97-protected-branch-gate-logs/$run_id"
private_key="$log_root/authority-private.pem"
public_key="$log_root/authority-public.pem"
integrity_envelope="$log_root/checkpoint-envelope.json"
mechanical_evidence="$log_root/ci-gate-evidence-v1.json"

fail() {
    printf '%s\n' "AD-T97-BRANCH: $1" >&2
    exit 1
}

expect_status() {
    local expected=$1
    shift
    set +e
    "$@" >"$log_root/command.out" 2>"$log_root/command.err"
    local observed=$?
    set -e
    [[ $observed -eq $expected ]] \
        || fail "expected status $expected but observed $observed."
}

for command in python3 openssl jq sha256sum; do
    command -v "$command" >/dev/null 2>&1 \
        || fail "required command is unavailable: $command"
done
[[ -f "$authority" && ! -L "$authority" ]] \
    || fail 'the reviewed protected branch authority is unavailable.'
[[ -f "$policy" && ! -L "$policy" ]] \
    || fail 'the protected branch policy is unavailable.'

umask 077
mkdir -p "$log_root"
openssl genpkey -algorithm ED25519 -out "$private_key" 2>"$log_root/keygen.err"
chmod 0600 "$private_key"
openssl pkey -in "$private_key" -pubout -out "$public_key" \
    2>"$log_root/public-key.err"
chmod 0644 "$public_key"

python3 -B - "$public_key" "$private_key" "$integrity_envelope" <<'PY'
import base64
import hashlib
import json
import pathlib
import subprocess
import sys
import tempfile

public_key = pathlib.Path(sys.argv[1])
private_key = pathlib.Path(sys.argv[2])
output = pathlib.Path(sys.argv[3])
der = subprocess.run(
    ["openssl", "pkey", "-pubin", "-in", str(public_key), "-outform", "DER"],
    check=True,
    stdout=subprocess.PIPE,
    stderr=subprocess.DEVNULL,
).stdout
checkpoint = {
    "authority_key_sha256": hashlib.sha256(der).hexdigest(),
    "contract_version": "integrity-checkpoint-v1",
    "database_count": 1,
    "databases": [{
        "event_streams": [],
        "governance_snapshots": [],
        "logical_id": "registry",
        "projects": [],
        "schema_version": 10,
        "semantic_sha256": "1" * 64,
    }],
    "epoch": 7,
    "previous_checkpoint_sha256": "2" * 64,
    "storage_schema_version": 10,
}
payload = json.dumps(
    checkpoint, ensure_ascii=True, allow_nan=False, sort_keys=True, separators=(",", ":")
).encode("ascii") + b"\n"
with tempfile.TemporaryDirectory(dir=output.parent) as temporary_root:
    input_path = pathlib.Path(temporary_root) / "checkpoint.json"
    signature_path = pathlib.Path(temporary_root) / "checkpoint.sig"
    input_path.write_bytes(payload)
    subprocess.run(
        [
            "openssl", "pkeyutl", "-sign", "-inkey", str(private_key), "-rawin",
            "-in", str(input_path), "-out", str(signature_path),
        ],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    signature = signature_path.read_bytes()
envelope = {
    "checkpoint": checkpoint,
    "contract_version": "signed-integrity-checkpoint-v1",
    "signature_base64": base64.b64encode(signature).decode("ascii"),
}
output.write_bytes(json.dumps(
    envelope, ensure_ascii=True, allow_nan=False, sort_keys=True, separators=(",", ":")
).encode("ascii") + b"\n")
PY

cat >"$mechanical_evidence" <<'EOF'
{
  "schema_version": "ci-gate-evidence-v1",
  "result": "passed",
  "authority_mode": "protected-digest",
  "wrapper_sha256": "3333333333333333333333333333333333333333333333333333333333333333",
  "source_sha256": "4444444444444444444444444444444444444444444444444444444444444444",
  "clean_target": true,
  "primary_wrapper_participation": "verified",
  "local_wrapper_override": "superseded",
  "cached_followup": "passed-without-primary-recompile",
  "signature": "none-local-mechanics-only"
}
EOF

printf '%s\n' 'AD-T97-BRANCH: accepting the clean protected evidence set.'
python3 -B "$authority" evaluate \
    --policy "$policy" \
    --mechanical-evidence "$mechanical_evidence" \
    --integrity-envelope "$integrity_envelope" \
    --integrity-public-key "$public_key" \
    --attestation-private-key "$private_key" \
    --output "$log_root/passed-attestation.json" \
    --observation cache-no-primary-recompile >"$log_root/passed.out"
python3 -B "$authority" verify \
    --policy "$policy" \
    --envelope "$log_root/passed-attestation.json" \
    --attestation-public-key "$public_key" >"$log_root/passed-verify.out"

mapfile -t blocking_codes < <(jq -r '.blocking_violation_codes[]' "$policy")
for code in "${blocking_codes[@]}"; do
    output="$log_root/blocked-$code.json"
    expect_status 1 python3 -B "$authority" evaluate \
        --policy "$policy" \
        --mechanical-evidence "$mechanical_evidence" \
        --integrity-envelope "$integrity_envelope" \
        --integrity-public-key "$public_key" \
        --attestation-private-key "$private_key" \
        --output "$output" \
        --violation "$code"
    expect_status 1 python3 -B "$authority" verify \
        --policy "$policy" \
        --envelope "$output" \
        --attestation-public-key "$public_key"
done

printf '%s\n' 'AD-T97-BRANCH: rejecting undefined codes and unprotected mechanical evidence.'
expect_status 2 python3 -B "$authority" evaluate \
    --policy "$policy" \
    --mechanical-evidence "$mechanical_evidence" \
    --integrity-envelope "$integrity_envelope" \
    --integrity-public-key "$public_key" \
    --attestation-private-key "$private_key" \
    --output "$log_root/undefined.json" \
    --violation undefined-code
[[ ! -e "$log_root/undefined.json" ]] \
    || fail 'an undefined violation produced an attestation.'

python3 -B - "$mechanical_evidence" "$log_root/local-evidence.json" <<'PY'
import json
import pathlib
import sys
value = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="ascii"))
value["authority_mode"] = "local-mechanics"
pathlib.Path(sys.argv[2]).write_text(json.dumps(value) + "\n", encoding="ascii")
PY
expect_status 2 python3 -B "$authority" evaluate \
    --policy "$policy" \
    --mechanical-evidence "$log_root/local-evidence.json" \
    --integrity-envelope "$integrity_envelope" \
    --integrity-public-key "$public_key" \
    --attestation-private-key "$private_key" \
    --output "$log_root/local-rejected.json"

printf '%s\n' 'AD-T97-BRANCH: rejecting altered policy, checkpoint and attestation signatures.'
python3 -B - "$policy" "$log_root/altered-policy.json" <<'PY'
import json
import pathlib
import sys
value = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="ascii"))
value["required_status_context"] = "agent-controlled/status"
pathlib.Path(sys.argv[2]).write_text(json.dumps(value) + "\n", encoding="ascii")
PY
expect_status 2 python3 -B "$authority" verify \
    --policy "$log_root/altered-policy.json" \
    --envelope "$log_root/passed-attestation.json" \
    --attestation-public-key "$public_key"

python3 -B - "$integrity_envelope" "$log_root/altered-checkpoint.json" <<'PY'
import json
import pathlib
import sys
value = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="ascii"))
value["checkpoint"]["epoch"] += 1
pathlib.Path(sys.argv[2]).write_text(json.dumps(value) + "\n", encoding="ascii")
PY
expect_status 2 python3 -B "$authority" evaluate \
    --policy "$policy" \
    --mechanical-evidence "$mechanical_evidence" \
    --integrity-envelope "$log_root/altered-checkpoint.json" \
    --integrity-public-key "$public_key" \
    --attestation-private-key "$private_key" \
    --output "$log_root/checkpoint-rejected.json"

python3 -B - "$log_root/passed-attestation.json" "$log_root/altered-attestation.json" <<'PY'
import json
import pathlib
import sys
value = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="ascii"))
value["attestation"]["source_sha256"] = "5" * 64
pathlib.Path(sys.argv[2]).write_text(json.dumps(value) + "\n", encoding="ascii")
PY
expect_status 2 python3 -B "$authority" verify \
    --policy "$policy" \
    --envelope "$log_root/altered-attestation.json" \
    --attestation-public-key "$public_key"

cat >"$log_root/evidence.json" <<EOF
{
  "contract_version": "protected-branch-gate-validation-v1",
  "physical_locators": "redacted",
  "clean_case": "passed",
  "blocking_code_count": ${#blocking_codes[@]},
  "defined_blocking_codes_rejected": true,
  "undefined_code_rejected": true,
  "local_mechanics_rejected": true,
  "altered_policy_rejected": true,
  "altered_checkpoint_rejected": true,
  "altered_attestation_rejected": true,
  "compiler_result_precedence": "preserved",
  "provider_branch_protection": "not-configured",
  "validation": "pass"
}
EOF

printf '%s\n' 'AD-T97-BRANCH: signed fail-closed mechanics and closed policy matrix passed.'
printf '%s\n' 'AD-T97-BRANCH: provider branch protection remains unconfigured.'
printf '%s\n' "AD-T97-BRANCH: logs=$log_root"
