#!/usr/bin/env bash

cd "$TARGET_PATH" || exit

sudo systemctl restart cdn.service
