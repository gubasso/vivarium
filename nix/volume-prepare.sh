# shellcheck shell=bash
if ! test -e /home/vivarium/.vivarium-first-boot; then
  chown vivarium:vivarium /home/vivarium
  chmod 0700 /home/vivarium
  touch /home/vivarium/.vivarium-first-boot
  chown vivarium:vivarium /home/vivarium/.vivarium-first-boot
fi
