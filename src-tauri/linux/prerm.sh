#!/bin/sh
# Removing the package takes our hosts section and browser policies out (an upgrade keeps them).
if [ "$1" = remove ] && [ -x /usr/bin/clockmanage ]; then
  /usr/bin/clockmanage --block-sites || true
fi
exit 0
