#!/usr/bin/env bash
set -e
echo "Running setup.sh"

# Scripts always recieve the same arguments from benchkit in the same order:
#
# pub struct ScriptArgs {
#     pub binary: String,
#     pub connect_address: String,
#     pub network: String,
#     pub snapshot_path: PathBuf,
#     pub tmp_data_dir: PathBuf,
# }

if [ "$#" -ne 5 ]; then
    echo "Error: Required arguments missing"
    exit 1
fi

# BINARY="$1"
# CONNECT_ADDRESS="$2"
# NETWORK="$3"
# SNAPSHOT_PATH="$4"
TMP_DATADIR="$5"
# echo "TMP_DATADIR: ${TMP_DATADIR}"

echo "Creating datadir ${TMP_DATADIR} with mkdir -p"
mkdir -p "${TMP_DATADIR}"

echo "Cleaning datadir contents from ${TMP_DATADIR}"
rm -Rf "${TMP_DATADIR:?}"/*
