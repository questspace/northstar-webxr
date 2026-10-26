@echo off
rem Starts the NorthStar hub with administrator rights (one UAC prompt), which lets it
rem restart the Ultraleap service and re-plug the XR50 over USB by itself whenever the
rem headset's USB link drops. Without this the hub works too, but recovery needs a click.
powershell -NoProfile -Command "Start-Process -Verb RunAs -FilePath '%~dp0target\release\northstar-hub.exe' -WorkingDirectory '%~dp0'"
