#!/bin/sh
# every nested test, one summary line each; full output in <test>.out
D=$(dirname "$0"); cd $D
for t in t-*.sh; do
  timeout 120 ./$t > ${t%.sh}.out 2>&1; ./unnest.sh >/dev/null 2>&1
  bad=$(grep -ciE 'panic|fail|error|timeout' ${t%.sh}.out)
  echo "$t bad=$bad | $(grep -v '^\s*$' ${t%.sh}.out | tail -1 | cut -c1-110)"
done
