# examples

The top level directories each contain an example of scies that can be assembled and run using the
scie-jump.

# Structure

There is a `prepare` script in Unix and Windows flavors that can be used to prepare and example.
It looks for a `.fetch` file in this directory with the same stem as the example directory name
and fetches each non-blank URL line in the file into the example directory.

Inside the example directory are lift manifests that can be used to build scies given a scie-jump
binary of the corresponding platform.

# Use

Currently, the best guide to each example are the CI jobs that execute them defined in
[the GH actions CI workflow](../.github/workflows/ci.yml) and [the CircleCI CI workflow](
../.circleci/config.yml). Those provide a series of shell commands to run to walk through the
example.