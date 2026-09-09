#!/usr/bin/env bash
# Administrative JSON arrives on stdin; the private receipt leaves on stdout.
set -euo pipefail
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec /usr/bin/python3 -I -B "$script_dir/official-task-operation.py" "$@"
