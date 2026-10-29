#!/bin/sh
set -eu
panel_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
# The IDE sandbox and the host may have different Python ABI versions.
for env_name in .venv-host .venv; do
  interpreter="$panel_dir/$env_name/bin/python"
  if [ -x "$interpreter" ] && "$interpreter" -c 'from PyQt6.QtWidgets import QApplication' >/dev/null 2>&1; then
    exec "$interpreter" "$panel_dir/main.py" "$@"
  fi
done
echo 'Create a venv with your system Python and install tools/api-panel/requirements.txt. See docs/local-api.md.' >&2
exit 1
