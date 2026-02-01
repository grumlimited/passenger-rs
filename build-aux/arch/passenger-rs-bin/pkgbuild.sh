#!/bin/sh

set -x

cp ../../../config.toml .
cp ../../../target/release/passenger-rs .

updpkgsums

makepkg -f
makepkg -f --printsrcinfo > .SRCINFO