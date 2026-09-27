# Releasing Turbo Desktop

A release reaches five places. A tag alone reaches one of them: 0.2.1 was tagged
and announced, and never arrived on RubyGems or npm.

Every part of a release carries the same version. The example below releases
v0.2.3.

## 1. Bump the version everywhere

| File | Field |
|---|---|
| `package.json`, `package-lock.json` | `version` (`npm version 0.2.3 --no-git-tag-version` does both) |
| `src-tauri/Cargo.toml` | `version`; then `cargo check` to update `Cargo.lock` |
| `src-tauri/tauri.conf.json` | `version` |
| `src/turbo-desktop.js` | `version` |
| `turbo_desktop-rails/lib/turbo_desktop/version.rb` | `VERSION`; then `bundle install` to update `Gemfile.lock` |
| `turbo_desktop-rails/CHANGELOG.md` | a new entry |
| `README.md`, `turbo_desktop-rails/README.md`, `docs/*.md`, `docs/index.html`, `site/index.html` | quoted versions |

`npm test` fails while any of these disagree with `package.json`, so a missed
file shows up there rather than after the release.

## 2. Merge

Open a pull request, wait for CI to pass, merge to `main`.

## 3. Tag

```bash
git switch main && git pull --ff-only
git tag v0.2.3
git push origin v0.2.3
```

The release workflow builds the installers and attaches them to a **draft**
GitHub Release. Review it and publish it by hand. If the workflow leaves more
than one draft for the tag, keep one and delete the rest.

## 4. Publish the gem

```bash
cd turbo_desktop-rails
gem build turbo_desktop-rails.gemspec
gem push turbo_desktop-rails-0.2.3.gem   # asks for a one-time password
rm turbo_desktop-rails-0.2.3.gem
```

## 5. Publish the npm package

```bash
npm whoami || npm login
npm pack --dry-run    # check the file list
npm publish
```

A published version cannot be replaced, and a yanked or unpublished number
cannot be used again. A mistake is fixed by the next version.

## 6. Check

```bash
gem search -r -e turbo_desktop-rails
npm view turbo-desktop version
```

Then scaffold an app from the registry, outside this checkout, and confirm it
resolves to the new gem:

```bash
cd "$(mktemp -d)" && npx --yes turbo-desktop@0.2.3 new scratch_app
grep "turbo_desktop-rails (" scratch_app/Gemfile.lock
```

## 7. Update the website

turbo-desktop.dev is deployed from the `main` branch of
[turbo_desktop_site](https://github.com/aguspe/turbo_desktop_site), a separate
repository. Update its version there, after the packages are published, so the
site never announces a version that cannot be installed.

The example app, [turbo_desktop_example_app](https://github.com/aguspe/turbo_desktop_example_app),
picks up the new gem with `bundle update turbo_desktop-rails`.
