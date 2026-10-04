#!/bin/sh
# desk: write labwc config every boot (idempotent).
mkdir -p /config/.config/labwc

cat > /config/.config/labwc/rc.xml <<'XML'
<?xml version="1.0"?>
<labwc_config>
  <theme>
    <cornerRadius>0</cornerRadius>
    <font><name>sans</name><size>10</size></font>
  </theme>
  <titleLayout>icon:title(m,c)</titleLayout>
  <root-menu>
    <item label="Acme">
      <action name="Execute" command="/usr/local/bin/launch-desk-acme.sh" />
    </item>
    <item label="Terminal">
      <action name="Execute" command="sh -c 'export WAYLAND_DISPLAY=wayland-0 HOME=/home/e4779; exec foot'" />
    </item>
    <item label="Browser">
      <action name="Execute" command="sh -c 'export WAYLAND_DISPLAY=wayland-0 HOME=/home/e4779; exec chromium'" />
    </item>
    <item label="pi-web">
      <action name="Execute" command="sh -c 'export WAYLAND_DISPLAY=wayland-0 HOME=/home/e4779; exec chromium http://192.168.1.3:8504'" />
    </item>
    <separator />
    <item label="Reconfigure">
      <action name="Reconfigure" />
    </item>
    <item label="Exit">
      <action name="Exit" />
    </item>
  </root-menu>
  <keybind key="A-F11"><action name="ToggleFullscreen"/></keybind>
  <keybind key="W-F10"><action name="ToggleMaximize"/></keybind>
  <keybind key="W-Return"><action name="Execute" command="foot"/></keybind>
</labwc_config>
XML

cat > /config/.config/labwc/menu.xml <<'XML'
<?xml version="1.0" encoding="UTF-8"?>
<openbox_menu>
  <menu id="root-menu">
    <item label="Acme">
      <action name="Execute" command="/usr/local/bin/launch-desk-acme.sh" />
    </item>
    <item label="Terminal">
      <action name="Execute" command="sh -c 'export WAYLAND_DISPLAY=wayland-0 HOME=/home/e4779; exec foot'" />
    </item>
    <item label="Browser">
      <action name="Execute" command="sh -c 'export WAYLAND_DISPLAY=wayland-0 HOME=/home/e4779; exec chromium'" />
    </item>
    <item label="pi-web">
      <action name="Execute" command="sh -c 'export WAYLAND_DISPLAY=wayland-0 HOME=/home/e4779; exec chromium http://192.168.1.3:8504'" />
    </item>
    <separator />
    <item label="Reconfigure">
      <action name="Reconfigure" />
    </item>
    <item label="Exit">
      <action name="Exit" />
    </item>
  </menu>
</openbox_menu>
XML

cat > /config/.config/labwc/autostart <<'SH'
#!/bin/sh
# acme on boot; manual relaunch: root menu -> Acme (no auto-respawn by design)
/usr/local/bin/launch-desk-acme.sh
SH
chown -R abc:abc /config/.config
