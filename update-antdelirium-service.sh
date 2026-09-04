#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

echo "Building the AntDelirium installation bundle..."
if [ "${SUDO_USER:-}" != "" ]; then
    sudo -u "$SUDO_USER" env "PATH=$PATH:/home/$SUDO_USER/.cargo/bin" make -C "$script_dir" system-installation-bundle
else
    make -C "$script_dir" system-installation-bundle
fi

echo "Starting the protected service upgrade..."
exec "$script_dir/packaging/systemd/upgrade-installed-service.sh"
