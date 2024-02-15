#!/bin/bash

# fail fast
set -e

# enable globstar for bash
shopt -s globstar

install_package() {
  if [ ! -x "$(command -v "$1")" ] && [ "$UID" -eq 0 ] && [ -x "$(command -v apt-get)" ]; then
    echo "installing gnu parallel"
    apt-get install --no-install-recommends -qqq -y "$2"
  elif [ ! -x "$(command -v "$1")" ] && [ "$UID" -ne 0 ]; then
    echo "missing gnu parallel. exiting."
    exit 1
  fi
}

check_diff() {
  set +e
  name=$(echo "$1" | cut -d'/' -f2-)
  if ! res="$(func_equiv "$1" "$2"/"$name")"; then
    echo "$name: msbts are NOT functionally equivalent"
    echo "$res"
    exit 1
  fi
  echo "$name: $res"
  set -e
}

check_diffs() {
  export -f check_diff
  parallel 'check_diff {1} {2}' ::: "$1"/**/*.msbt ::: "$2"
}

main() {
  [[ "$CI_JOB_ID" != "" ]] && cp target/release/msyt /usr/local/bin/

  # make temp dir
  tmp=$(mktemp -d)

  # remove dir if we fail
  trap "rm -rf -- $tmp" EXIT

  # move to temp dir
  cd "$tmp" || exit

  install_package wget wget
  install_package parallel parallel
  install_package xz xz-utils

  if [ ! -x "$(command -v func_equiv)" ]; then
    echo "installing func_equiv"
    wget https://kyleclemens.com/assets/msyt/func_equiv.xz
    xz -d func_equiv.xz
    chmod +x func_equiv
    mv func_equiv /usr/local/bin/
  fi

  echo "downloading vanilla msbts"

  # download vanilla switch msbts
  wget https://kyleclemens.com/assets/msyt/switch_language_msbts.tar.xz

  # download vanilla wiiu msbts
  wget https://kyleclemens.com/assets/msyt/wiiu_language_msbts.tar.xz

  echo "extracting"

  # extract them
  for p in switch wiiu; do
    mkdir "$p"
    cd "$p" || exit
    tar xvf "../${p}_language_msbts.tar.xz"
    cd .. || exit
  done

  # remove archive
  rm ./*_language_msbts.tar.xz

  # generate msyts
  echo "generating switch msyts"
  msyt export -d switch
  echo "generating wiiu msyts"
  msyt export -d wiiu

  # generate switch msbts
  echo "creating switch msbts"
  msyt create -dp switch -o msbt_switch switch
  # generate wiiu msbts
  echo "creating wiiu msbts"
  msyt create -dp wiiu -o msbt_wiiu wiiu

  # check that switch msbts are functionally equivalent to vanilla
  echo "comparing switch msbts"
  check_diffs "msbt_switch" "switch"

  # check that wiiu msbts are functionally equivalent to vanilla
  echo "comparing wiiu msbts"
  check_diffs "msbt_wiiu" "wiiu"

  echo "done"
}

main
