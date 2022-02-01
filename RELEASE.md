To make a new release:

- update version in the `Cargo.toml` files

```bash
$ git checkout -b new_release
$ sed -i .bak 's/^version.*/version = "0.1.20"/' Cargo.toml */Cargo.toml
# or edit them manually with `vi Cargo.toml */Cargo.toml`

# build to update `Cargo.lock`
$ cargo build

$ git add Cargo.toml Cargo.lock */Cargo.toml

$ git commit -m 'Update version.'

$ git push
```

Now create a PR with the `new_release` branch. Merge it.

```
$ git checkout main
$ git pull
$ git tag 0.1.20
$ git push --tags
```

The `0.1.20` tag on GitHub will cause the artifacts to be uploaded to crates.io, pypi.org and npmjs.com.
