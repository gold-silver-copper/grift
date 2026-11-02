#!/bin/sh
# Runs query on the files in data/ and compares each output and exit status with the expected.
# GRIFT names the runner; run from anywhere.
cd "$(dirname "$0")" || exit 2
G=${GRIFT:?set GRIFT to the runner}
failed=0
passed=0
# check NAME STATUS EXPECTED -- ARGUMENT...: stdout and stderr together.
check() {
  name=$1 status=$2 expected=$3
  shift 4
  actual=$("$G" main.grift "$@" 2>&1)
  got=$?
  if [ "$actual" = "$expected" ] && [ "$got" = "$status" ]; then
    passed=$((passed + 1))
  else
    failed=$((failed + 1))
    printf 'FAIL %s (status %s, expected %s)\n--- expected\n%s\n--- actual\n%s\n' "$name" "$got" "$status" "$expected" "$actual"
  fi
}

# 1. CSV: quoted fields, embedded commas, quotes and a newline, a header row; \r\n and a blank line.
check "quoted fields" 0 'name  city    age   salary  note
----  ------  ---  -------  ---------------------
Ada   London   36   5200.5  likes "tea", and cake
Bob   Paris    28   4100.0
Cleo  London   41   6100.0  multi\nline
Dan   Berlin   28  3900.25  plain
Eve   Paris    33   4800.0  quoted number
Finn  Berlin   52   7000.0  -' -- '' data/people.csv
check "crlf and a blank line" 0 'item           qty  price
-------------  ---  -----
widget, large   10    2.5
bolt             9    0.1
nut            100   0.05
say "hi"         9    1.0' -- '' data/crlf.csv
check "csv round trip" 0 'item,qty,price
"widget, large",10,2.5
bolt,9,0.1
nut,100,0.05
"say ""hi""",9,1.0' -- --csv '' data/crlf.csv
check "multi-line field as csv" 0 'name,note
Cleo,"multi
line"' -- --csv '(where (same? name "Cleo")) (select name note)' data/people.csv
check "unterminated quote" 1 'query: data/unterminated.csv: line 2: a quoted field has no closing quote' -- '' data/unterminated.csv
check "short record" 1 'query: data/short.csv: record 3 has 1 fields and the header 2' -- '' data/short.csv

# 2. Select, rename, compute; filter by a predicate over named columns.
check "select and rename" 0 'name  town     monthly
----  ------  --------
Ada   London   433.375
Cleo  London  508.3333
Eve   Paris      400.0
Finn  Berlin  583.3333' -- '(where (greater-than? age 30)) (select name (as town city) (as monthly (divide salary 12.0)))' data/people.csv
check "filter with and" 0 'name  age
----  ---
Bob    28
Dan    28' -- '(where (and!? (less-than? age 30) (not? (same? city "London")))) (select name age)' data/people.csv
check "column by string" 0 'item  qty  price
----  ---  -----
bolt    9    0.1' -- '(where (string-starts-with? (column "item") "b"))' data/crlf.csv

# 3. Group by with count, sum, average, minimum, maximum.
check "group-by" 0 'city    count  sum-salary  average-age  minimum-salary  maximum-age
------  -----  ----------  -----------  --------------  -----------
London      2     11300.5         38.5          5200.5           41
Paris       2      8900.0         30.5          4100.0           33
Berlin      2    10900.25         40.0         3900.25           52' -- '(group-by city (count) (sum salary) (average age) (minimum salary) (maximum age))' data/people.csv
check "group-by two keys, named aggregate" 0 'city    age  people
------  ---  ------
Berlin   28       1
Berlin   52       1
London   36       1
London   41       1
Paris    28       1
Paris    33       1' -- '(group-by (city age) (as people (count))) (sort-by city age)' data/people.csv
check "sum of text" 1 'query: in (group-by city (sum note)): not a number: "likes \"tea\", and cake"' -- '(group-by city (sum note))' data/people.csv

# 4. Sort by several columns, ascending and descending, numbers as numbers.
check "numbers as numbers" 0 'item           qty  price
-------------  ---  -----
say "hi"         9    1.0
bolt             9    0.1
widget, large   10    2.5
nut            100   0.05' -- '(sort-by qty (descending item))' data/crlf.csv
check "descending then limit" 0 'name,age
Finn,52
Cleo,41
Ada,36' -- --csv '(sort-by (descending age) name) (select name age) (limit 3)' data/people.csv
check "sort after group" 0 'city       total
------  --------
London   11300.5
Berlin  10900.25
Paris     8900.0' -- '(group-by city (as total (sum salary))) (sort-by (descending total))' data/people.csv

# 5. Output: aligned text (above) or CSV; a column that doesn't exist is an error naming it.
check "unknown column in select" 1 'query: in (select name agee): no column named agee; the columns are name, city, age, salary, note' -- '(select name agee)' data/people.csv
check "unknown column in where" 1 'query: in (where (less-than? agee 3)): no column named agee; the columns are name, city, age, salary, note' -- '(where (less-than? agee 3))' data/people.csv
check "unknown column after rename" 1 'query: in (sort-by city): no column named city; the columns are name, town' -- '(select name (as town city)) (sort-by city)' data/people.csv
check "unknown clause" 1 'query: not a clause; the clauses are select, where, group-by, sort-by and limit: (selec name)' -- '(selec name)' data/people.csv
check "where not boolean" 1 'query: in (where qty): a where test must give #true or #false: qty, not 10' -- '(where qty)' data/crlf.csv
check "query that does not read" 1 'query: the query doesn'"'"'t read at line 1, column 13' -- '(select name' data/people.csv
check "missing file" 2 'query: nope.csv: file-error "nope.csv" "No such file or directory (os error 2)"' -- '' nope.csv
check "usage" 2 'query: usage: main.grift [--csv] QUERY FILE.csv' -- ''

echo "$passed passed, $failed failed"
[ "$failed" = 0 ]
