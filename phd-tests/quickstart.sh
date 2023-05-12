#!/bin/bash

PHD_QUICKSTART_DIR=/tmp/propolis-phd
if [ -f "$PHD_QUICKSTART_DIR" ]; then
	echo "$PHD_QUICKSTART_DIR exists and is not a directory"
	exit 1
fi

if [ ! -d "$PHD_QUICKSTART_DIR" ]; then
	mkdir $PHD_QUICKSTART_DIR
fi

pfexec cargo run --profile=phd -p phd-runner -- \
	run \
	--artifact-toml-path ./artifacts.toml \
	--tmp-directory $PHD_QUICKSTART_DIR \
	--artifact-directory $PHD_QUICKSTART_DIR \
	--propolis-server-cmd $1
