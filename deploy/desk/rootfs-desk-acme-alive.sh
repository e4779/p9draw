#!/bin/sh
ps -eo stat,comm | awk '$2=="acme" && $1 !~ /Z/ {found=1} $2=="p9draw-server" && $1 !~ /Z/ {found2=1} END {exit !(found && found2)}'
