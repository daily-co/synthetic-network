#!/usr/bin/env bash

conf=$1
function reload {
    echo "SIGHUP: reload"
    cat $conf
}
trap reload HUP
while true; do sleep .1; done
