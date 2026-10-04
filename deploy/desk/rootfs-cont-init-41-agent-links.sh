#!/bin/sh
# pi via the host home mounted at the same path; bun is in the mounted .bun.
ln -sf /home/e4779/.bun/bin/bun /usr/local/bin/bun
printf '#!/bin/sh\nexec /home/e4779/.bun/bin/bun /home/e4779/.bun/install/global/node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js "$@"\n' > /usr/local/bin/pi
chmod +x /usr/local/bin/pi /usr/local/bin/bun
