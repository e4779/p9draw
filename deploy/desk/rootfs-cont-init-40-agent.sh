#!/bin/sh
# expose the HOST pi agent inside the desk terminal (native, no ssh):
# ~/.bun and ~/.pi are bind-mounted by run-desk.sh (same uid 1000).
ln -sf /home/abc/.bun/bin/pi /usr/local/bin/pi
ln -sf /home/abc/.bun/bin/bun /usr/local/bin/bun
ln -sf /home/abc/.bun/bin/bunx /usr/local/bin/bunx
