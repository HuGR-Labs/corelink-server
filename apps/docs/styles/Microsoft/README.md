# Microsoft Writing Style Guide (Vale styles)

This directory holds the vendored Vale rules for the Microsoft Writing
Style Guide, used by `vale --config .vale.ini` and the `docs-ci`
workflow.

The full ruleset is downloaded by the CI workflow (`vale sync`) so that
local checkouts stay light. The placeholder file `placeholder.yml`
ensures the directory exists in git for `vale` to discover the style
location even before `vale sync` has run locally.

To populate locally:

```bash
brew install vale            # or see https://vale.sh/docs/vale-cli/installation/
cd apps/docs
vale sync                    # downloads Microsoft + Vale packages
pnpm vale                    # lint the docs/ tree
```
