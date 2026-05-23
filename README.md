# yaml-seeder

`yaml-seeder` is a small CLI for applying and exporting YAML seed data against
MySQL-compatible databases such as MySQL and TiDB.

## Install

Download a prebuilt binary from
[GitHub Releases](https://github.com/quantum-box/yaml-seeder/releases):

```bash
# macOS Apple Silicon example
curl -L -o yaml-seeder.tar.gz \
  https://github.com/quantum-box/yaml-seeder/releases/latest/download/yaml-seeder-v0.1.0-aarch64-apple-darwin.tar.gz
tar -xzf yaml-seeder.tar.gz
sudo install yaml-seeder-v0.1.0-aarch64-apple-darwin/yaml-seeder /usr/local/bin/yaml-seeder
```

Or install from source:

```bash
cargo install --git https://github.com/quantum-box/yaml-seeder
```

Release artifacts are built automatically for:

- `x86_64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

## Usage

Create a seed file:

```bash
yaml-seeder create add-users --directory scripts/seeds
```

Apply a file or directory:

```bash
DATABASE_URL=mysql://root@127.0.0.1:15000 \
  yaml-seeder apply scripts/seeds
```

Apply against a named environment:

```bash
DEV_DATABASE_URL=mysql://root@127.0.0.1:15000 \
  yaml-seeder apply dev scripts/seeds
```

Export tables:

```bash
DATABASE_URL=mysql://root@127.0.0.1:15000 \
  yaml-seeder export ./seed.yaml --table app.users --pretty
```

## Seed Format

```yaml
version: 1
tables:
  - name: app.users
    mode: upsert-update
    rows:
      - id: user_1
        name: Alice
```

`mode` defaults to `upsert-update`. Use `insert` for plain inserts.
