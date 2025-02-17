# Benchkit

A benchmarking kit which can read one or more benchmarks from a configuration
file, execute them and write the results to a database.

Currently supported configuration values:

## global

- database          # the database file to store results in
- Hyperfine         # hyperfine aruments
  - warmup_count
  - runs
  - shell
  - name
  - command
  - setup
  - prepare
  - conclude
  - cleanup
- wrapper           # a command to wrap the `hyperfine` command, e.g. `taskset`

## benchmarks

- name
- env               # environment variables
- hyperfine         # specific hyperfine benchmark commands and arguments.
                    # these override globals


Additionally the `benchkit` binary supports passing a `pr_number` and `run_id`
(e.g. github workflow run id) as cli arguments to be added to the database
metadata.
