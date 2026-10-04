#!/bin/sh
# desk: write labwc config into /config every boot (idempotent).
mkdir -p /config/.config/labwc
cat > /config/.config/labwc/rc.xml <<'XML'
<?xml version="1.0"?>
<labwc_config>
  <theme>
    <cornerRadius>0</cornerRadius>
    <font><name>sans</name><size>10</size></font>
  </theme>
  <titleLayout>icon:title(m,c)</titleLayout>
  <keybind key="A-F11"><action name="ToggleFullscreen"/></keybind>
  <keybind key="W-F10"><action name="ToggleMaximize"/></keybind>
  <keybind key="W-Return"><action name="Execute" command="foot"/></keybind>
  <root-menu>
    <item label="Acme" execute="/usr/local/bin/launch-desk-acme.sh" />
    <item label="Terminal" execute="foot" />
    <item label="Browser" execute="chromium" />
    <item label="pi-web" execute="chromium http://192.168.1.3:8504" />
    <item label="Reconfigure" execute="labwc -r" />
  </root-menu>
</labwc_config>
XML
cat > /config/.config/labwc/autostart <<'SH'
#!/bin/sh
/usr/local/bin/launch-desk-acme.sh
nohup /usr/local/bin/desk-acme-keepalive.sh >/dev/null 2>&1 &
SH
chmod +x /config/.config/labwc/autostart
chown -R abc:abc /config/.config
