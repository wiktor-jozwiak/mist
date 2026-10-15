#!/bin/sh
i=0; while [ $i -lt 3000 ]; do printf '\033[4%dm  \033[0m' $((i % 7 + 1)); i=$((i+1)); done; sleep 1000
