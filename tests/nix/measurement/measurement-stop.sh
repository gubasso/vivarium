# shellcheck shell=bash
set -u
exec >/dev/console 2>&1
echo "VIVARIUM_MEASUREMENT_COMPLETE=yes legs=${VIVARIUM_MEASUREMENT_LEGS}"
systemctl poweroff
