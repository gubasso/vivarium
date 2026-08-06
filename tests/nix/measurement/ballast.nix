let
  bash = builtins.storePath "@bash@";
  coreutils = builtins.storePath "@coreutils@";
  each = builtins.getEnv "BALLAST_FILE_BYTES";
  mib = builtins.getEnv "BALLAST_MIB";
  chatty = builtins.getEnv "BALLAST_CHATTY";
in
derivation {
  name = "vivarium-ballast-" + (builtins.getEnv "BALLAST_NAME");
  system = "@system@";
  builder = bash + "/bin/bash";
  args = [
    "-c"
    (
      "export PATH="
      + coreutils
      + "/bin\n"
      + "set -e\n"
      + "mkdir -p \"$out\"\n"
      + "total=$(( "
      + mib
      + " * 1024 * 1024 ))\n"
      + "each="
      + each
      + "\n"
      + "n=$(( total / each ))\n"
      + "i=0; dir=0\n"
      + "while [ $i -lt $n ]; do\n"
      + "  if [ $(( i % 1000 )) -eq 0 ]; then dir=$(( i / 1000 )); mkdir -p \"$out/d$dir\"; fi\n"
      + "  head -c $each /dev/urandom > \"$out/d$dir/f$i\"\n"
      + "  i=$(( i + 1 ))\n"
      + "  if [ "
      + chatty
      + " = yes ] && [ $(( i % 64 )) -eq 0 ]; then echo \"ballast $i/$n\"; fi\n"
      + "done\n"
    )
  ];
}
