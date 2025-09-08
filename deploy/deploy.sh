#!/usr/bin/env bash

cd /opt/production/cdn || exit

sudo systemctl restart cdn.service
