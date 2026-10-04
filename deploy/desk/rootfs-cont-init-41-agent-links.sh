#!/bin/sh
# expose the HOST pi agent inside the desk terminal (native, no ssh):
# ~/.bun and ~/.pi are bind-mounted by run-desk.sh (same uid 1000).
printf '#!/bin/sh\nexport HOME=/home/abc\nexport BUN_INSTALL=/home/abc/.bun\nexec /home/abc/.bun/bin/bun /home/abc/.bun/install/global/node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js "$@"\n' > /usr/local/bin/pi
printf '#!/bin/sh\nexport HOME=/home/abc\nexport BUN_INSTALL=/home/abc/.bun\nexec /home/abc/.bun/bin/bun "$@"\n' > /usr/local/bin/bun
chmod +x /usr/local/bin/pi /usr/local/bin/bun
