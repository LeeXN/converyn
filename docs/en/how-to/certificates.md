# Personal certificates and tokens

[English](certificates.md) | [简体中文](../../zh-CN/how-to/certificates.md)

OP issues and rotates paired credentials through CLI; GUI supports enable identities/disable and access management. Management uses named administrator credentials; issuance additionally needs the protected local user-authority. If installation already created Alice, continue at browser import; creation commands apply only to new identities.

For initial deployment use the [initializer](../reference/init-script.md): `import` reuses the installer OP, `user` issues personal credentials, grants a role and imports the context, and `browser` exports the recipient P12/token/CA and login instructions. See the [walkthrough](../tutorials/quickstart.md) for commands and private-directory preparation. The underlying CLI lifecycle operations remain below; do not create the same person twice.

```bash
converyn config import /secure/qa-op/config.json --name qa-op --use
export CONVERYN_AUTHORITY=/secure/converyn-qa/user-authority
converyn create identities --name Alice --valid-for 365d --output /secure/qa-alice
converyn get identities
converyn get credentials
```

Creation returns a stable person ID for --id/--identity. New people have no grants by default; --super-admin creates a super administrator. See [workflow access](access.md) to grant ordinary access. Output directories must be new and use mode 0700. Keys, certificate, token, config.json, credential.json and operation.json are protected files. Lists never expose private keys or tokens.

Personal lifetime defaults to 365 days, minimum five minutes, maximum 1095 days. --valid-for accepts s/m/h/d; alternatively --expires-at accepts RFC3339 with timezone. Requests exceeding issuer validity are rejected, not silently shortened. Node/controller lifetimes remain controlled by --leaf-lifetime-seconds rather than personal defaults.

## Browser import

```bash
converyn export credentials --config /secure/qa-alice/config.json --current \
  --output /secure/qa-alice/browser.p12
```

Optional PKCS#12 export requires OpenSSL on OP's machine. Enter the export password interactively or use --password-file pointing to a protected file. Never put passwords in arguments. Import the package into the browser computer's personal certificate store, verify/trust the public CA, and choose that certificate for the controller's HTTPS origin. Windows uses certmgr.msc. Log in with the control-plane name and matching token file. The export password is only for importing the package.

## Update, rotate and revoke

```bash
converyn update identities USER_ID --name NewName
converyn rotate credentials --identity USER_ID --valid-for 730d \
  --output /secure/qa-alice-new
converyn rotate credentials --identity USER_ID --valid-for 365d \
  --overlap 24h --output /secure/qa-alice-next
converyn revoke credentials CREDENTIAL_ID
converyn disable identities USER_ID
converyn enable identities USER_ID
```

Certificates cannot change in place. Rotation generates a new certificate, key and token while retaining person ID. Cross-pairing old/new credentials fails. Old credentials are revoked immediately by default; explicit overlap allows at most seven days and expires in authentication without a cleanup job. Repeated rotation cannot extend an old deadline. Replace both browser certificate and token.

Delete revokes one pair and retains evidence. Disable blocks every pair for a person; the last enabled administrator identity cannot be disabled. Preserve another administrator and offline recovery material. Revocation does not cancel accepted Runs.

## Interrupted operations

The new private bundle is persisted before registration. Recover an unknown outcome using:

```bash
converyn resume credentials --output /secure/qa-alice-new
```

It reuses operation.json and checks whether the new pair is active, without minting another key or extending overlap. Keep the original output directory.

Controllers store public certificates, fingerprints and token hashes, and verify trust chains before registration. Both mTLS and API binding are required. Streaming logs revalidate credentials. Audit excludes bearer values, private keys and export passwords.

## Administrator recovery

Only when all named administrator credentials are unavailable, and bootstrap credentials remain valid, OP may use the protected bootstrap config to rotate credentials for an existing enabled administrator. It cannot create routine users or modify workflows. Use rotate credentials with --config /secure/converyn-qa/admin/config.json, --authority /secure/converyn-qa/user-authority and the existing --identity. The stable person ID and audit remain. Routine administration uses named credentials.

## Details, locations and export

Use `get identities NAME_OR_UID` for a person's associated credentials, `get credentials --identity NAME_OR_UID` to filter, and `get credentials CREDENTIAL_ID` for validity, revocation, fingerprint, serial, registry storage and matching local credential location. Names must be unique. The controller stores public certificates and token digests; private keys, bearer values and original workstation directories remain client-held.

```bash
converyn get identities OP
converyn get credentials --identity QA
converyn get credentials CREDENTIAL_ID
converyn export credentials --id CREDENTIAL_ID --output ./public.crt
converyn export credentials --current --output ./browser.p12
converyn --context qa export credentials --current --output ./qa.p12
converyn config export --output ./personal-context.json
```

`--id` exports the registered public PEM certificate only. `--current` exports a PKCS#12 bundle with the selected local private key, including credentials embedded in an imported context; the original issuance directory is not required. PKCS#12 requires an export password and does not contain the API bearer token. `config export` writes a protected portable JSON file containing the matching token, key and certificates. Keep both private. Sources `--id`, `--current` and `--ca` are mutually exclusive and exports refuse existing files and symlinks. Local location information describes only the selected config, not copies on other workstations.

## Export CA and generate GUI login instructions

```bash
converyn export credentials --ca --output ./ca.crt
converyn --context single export credentials --ca --output ./single-ca.crt
converyn auth gui-login --output-dir ./gui-login
converyn --config /secure/qa-alice/config.json auth gui-login \
  --output-dir ./alice-gui --with-credentials
```

`--ca` exports every public CA in the selected configuration's PEM trust bundle, reporting fingerprints and expiry without exporting a CA private key. Existing files and symlinks are refused.

`gui-login` confirms a named personal identity over mTLS and writes a new directory. Default contents are `README.md` and `ca.crt`: actual endpoints, control-plane name, person/role, certificate fingerprints/expiry, Windows import and browser login steps, and troubleshooting. Chinese is the default; select English with `--language en`. No identities are created or rotated.

Opting into `--with-credentials` also writes `client.p12` and `token.txt`. Select the intended person's configuration rather than distributing an OP administrator's credentials. This is a private package (0700 directory, 0600 files). OpenSSL prompts for the P12 password, or reads a private `--password-file`. Keys, bearer values and passwords never appear in the README or command output; the password is not included in the package and must be delivered separately. Existing directories, including empty directories and symlinks, are refused. Failed generation never publishes a partial directory. The output parent must exist.
