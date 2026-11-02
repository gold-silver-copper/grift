#!/bin/sh
# Runs the markdown converter's cases, then times it on a document of 1,080 lines.
# GRIFT names the runner; run from anywhere.
set -e
dir=$(dirname "$0")
grift=${GRIFT:?set GRIFT to the runner}
"$grift" $dir/main.grift --test $dir/tests.grift
big=${TMPDIR:?set TMPDIR}/markdown-big.md
for i in $(seq 40); do cat $dir/sample.md; done > "$big"
start=$(date +%s.%N)
"$grift" $dir/main.grift < "$big" > "$big.html"
end=$(date +%s.%N)
echo "$(wc -l < "$big") lines of Markdown to $(wc -l < "$big.html") lines of HTML in $(awk "BEGIN { printf \"%.3f\", $end - $start }") s"
rm -f "$big" "$big.html"
