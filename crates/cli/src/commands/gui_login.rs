//! Recipient login documentation. Secrets are separate, opt-in, private files.
use super::*;
use converyn_runtime::{
    credentials::pki,
    interface::{client::Config, profile::ContextConfig},
};
use serde_json::{Value, json};
use std::{fs, os::unix::fs::DirBuilderExt, path::Path};
use x509_parser::prelude::FromDer;

#[derive(Clone, Copy, clap::ValueEnum)]
pub(crate) enum Language {
    #[value(name = "zh-CN")]
    Chinese,
    #[value(name = "en")]
    English,
}
#[derive(clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    pub connection: super::operator::Connection,
    /// New delivery directory; publishes README.md and ca.crt.
    #[arg(long)]
    output_dir: PathBuf,
    /// Also include the selected local user's private client.p12 and token.txt.
    #[arg(long)]
    with_credentials: bool,
    /// Private password file for client.p12; omit for an interactive OpenSSL prompt.
    #[arg(long, requires = "with_credentials")]
    password_file: Option<PathBuf>,
    /// Language of the generated login instructions.
    #[arg(long, value_enum, default_value = "zh-CN")]
    language: Language,
}
fn date(ms: u64) -> Result<String> {
    Ok(
        time::OffsetDateTime::from_unix_timestamp(i64::try_from(ms / 1000)?)?
            .format(&time::format_description::well_known::Rfc3339)?,
    )
}
fn md(value: &str) -> String {
    value
        .chars()
        .flat_map(|c| match c {
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '|' => vec!['\\', c],
            c if c.is_control() => vec![' '],
            _ => vec![c],
        })
        .collect()
}
fn certificate(config: &Config) -> Result<(String, String, u64)> {
    let pem = if let Some(c) = &config.embedded {
        c.certificate_pem.clone()
    } else {
        fs::read_to_string(&config.tls.cert)?
    };
    let info = pki::info(&pem)?;
    ensure!(
        !info.is_ca && info.client_auth,
        "GUI login requires a personal client certificate"
    );
    let der = rustls_pemfile::certs(&mut pem.as_bytes())
        .next()
        .context("client certificate missing")??;
    let (_, cert) = x509_parser::certificate::X509Certificate::from_der(der.as_ref())
        .map_err(|_| anyhow::anyhow!("invalid client certificate"))?;
    let subject = cert
        .subject()
        .iter_common_name()
        .next()
        .and_then(|v| v.as_str().ok())
        .unwrap_or("unknown")
        .to_owned();
    Ok((subject, info.fingerprint, info.expires_ms))
}
struct LoginIdentity<'a> {
    subject: &'a str,
    fingerprint: &'a str,
    expires: u64,
    name: &'a str,
    role: &'a str,
}
fn document(
    config: &Config,
    language: Language,
    private: bool,
    identity: &LoginIdentity<'_>,
    ca: &[Value],
) -> Result<String> {
    let LoginIdentity {
        subject,
        fingerprint,
        expires,
        name,
        role,
    } = *identity;
    let urls = config
        .servers
        .iter()
        .map(|v| {
            let mut url = reqwest::Url::parse(v).expect("validated controller origin");
            url.query_pairs_mut()
                .append_pair("cluster", &config.cluster);
            format!("- {}", md(url.as_str()))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let fingerprints = ca
        .iter()
        .map(|v| {
            format!(
                "- SHA256: `{}` · {}",
                v["sha256"].as_str().unwrap_or_default(),
                date(v["expires_ms"].as_u64().unwrap_or(0)).unwrap_or_else(|_| "unknown".into())
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let cluster = md(&config.cluster);
    let subject = md(subject);
    let name = md(name);
    let role = md(role);
    let expiry = date(expires)?;
    Ok(match language {
        Language::Chinese => format!(
            r#"# Converyn GUI 登录说明

## 登录信息

- 控制面名称：{cluster}
- 登录人员：{name}
- 权限：{role}
- 个人证书主体 / UID：{subject}
- 个人证书 SHA256：`{fingerprint}`
- 个人证书到期时间（UTC）：{expiry}
- 本说明按所选客户端配置生成；账户是否停用、凭据是否撤销，以及服务器连通性，以实际登录验证为准。

访问地址（任选可访问的一项）：

{urls}

## 文件与交付

{files}

CA 文件 `ca.crt` 的证书指纹与到期时间（UTC）：

{fingerprints}

## 登录步骤

1. 向管理员通过独立渠道核对 CA 指纹。Windows 可运行 `certmgr.msc`，将 `ca.crt` 中的 CA 导入当前用户的“受信任的根证书颁发机构”；若包含中间 CA，将其放入“中间证书颁发机构”。
2. 将本人配套的 `client.p12` 导入当前用户的“个人”证书库，输入管理员另行提供的 P12 导出密码，确认个人证书带有私钥。
3. Chrome / Edge 使用系统证书库；如果所用浏览器使用独立证书库，在该浏览器的证书管理器中分别导入 CA 和个人 P12。
4. 用浏览器打开上面的 HTTPS 地址，在客户端证书选择框中选择本人的证书。
5. 页面“控制面名称”填写 **{cluster}**，“令牌”填写管理员为这张个人证书配套提供的原始令牌（打包交付时为 `token.txt` 内容），不要加 `Bearer ` 前缀，然后点击“连接”。
6. 核对登录后显示的姓名和权限。引导 admin、Node 和控制器凭据不能替代日常 GUI 使用的具名人员凭据。

P12 导出密码只用于导入证书，不是 GUI 登录令牌。证书与令牌必须属于同一套凭据。轮换凭据后，按管理员指示同时更新浏览器证书和令牌。

## 常见问题

- `ERR_BAD_SSL_CLIENT_AUTH_CERT`：检查导入了正确的个人 P12、证书包含私钥且未过期，并选择了正确证书。
- 页面加载前证书错误：核对地址、CA 信任和浏览器选择；必要时关闭所有浏览器进程后重新选择有效证书，不要跳过服务器证书验证。
- `Unauthorized` / 未授权：核对控制面名称、配套令牌、人员是否停用，以及凭据是否已撤销。
- 已轮换但仍选择旧证书：移除对应旧个人证书或清除系统 SSL 状态，再重新打开浏览器。

## 保管与退出

CA 和本说明不包含私钥或令牌。个人 P12 与令牌只交付给对应人员本人，使用受保护渠道，P12 密码另行发送。不要把管理员凭据交给普通执行人员。令牌保留在当前标签页的会话存储中，刷新后先向服务器核验身份再恢复页面；退出或凭据失效后清除。浏览器恢复已关闭标签页时可能恢复会话。
"#,
            files = if private {
                "本目录包含：\n\n- `README.md`：本说明，无令牌、私钥或密码。\n- `ca.crt`：公开 CA 证书，可用于验证服务器。\n- `client.p12`：当前选中用户的个人证书和私钥，属于私有凭据。\n- `token.txt`：与个人证书配套的 API 登录令牌，属于私有凭据。\n\n此目录是指定人员的私有交付包，不可作为公开资料分发。P12 密码不会保存到目录中，请单独交付。"
            } else {
                "本目录只包含 `README.md` 和公开 CA 文件 `ca.crt`。个人 `client.p12`、配套原始令牌和 P12 密码需要由管理员另行提供。本说明不创建、不轮换任何账户或凭据。"
            }
        ),
        Language::English => format!(
            r#"# Converyn GUI login guide

## Connection

- Control-plane name: {cluster}
- Person: {name}
- Role: {role}
- Personal certificate subject / UID: {subject}
- Personal certificate SHA256: `{fingerprint}`
- Certificate expiry (UTC): {expiry}
- Generated from the selected client configuration. Account status, credential revocation and connectivity must be confirmed at login.

HTTPS addresses (choose an accessible endpoint):

{urls}

## Files and delivery

{files}

Certificates in `ca.crt`, with SHA256 fingerprints and expiry (UTC):

{fingerprints}

## Sign in

1. Confirm the CA fingerprints with your administrator through an independent channel. On Windows, open `certmgr.msc`: import the root CA into the current user's Trusted Root Certification Authorities and any intermediate CA into Intermediate Certification Authorities.
2. Import your own `client.p12` into the current user's Personal certificate store. Enter the separately supplied P12 export password and confirm the certificate has its private key.
3. Chrome / Edge use the system certificate store. If your browser uses a separate store, import the CA and personal P12 through that browser's certificate manager.
4. Open an HTTPS address above and select your personal client certificate when prompted.
5. Enter **{cluster}** as the control-plane name. Enter the original matching token supplied for this certificate (the contents of `token.txt` in a private delivery package), without a `Bearer ` prefix, and click Connect.
6. Confirm the displayed name and role. Bootstrap admin, Node and controller credentials do not replace a named person's GUI credentials.

The P12 password is for certificate import, not GUI authentication. The certificate and token must be a matching pair. After rotation, replace both as instructed by the administrator.

## Troubleshooting

- `ERR_BAD_SSL_CLIENT_AUTH_CERT`: check the selected personal certificate, private key, expiry and correct P12 import.
- TLS error before the page loads: check address, CA trust and client-certificate selection; restart all browser processes if needed. Keep server-certificate verification enabled.
- `Unauthorized`: check the control-plane name, matching token, disabled identity and revoked credential.
- Old certificate still selected after rotation: remove the corresponding retired personal certificate or clear the system SSL state, then reopen the browser.

## Handling

This guide and the CA contain no private key or bearer value. Deliver the personal P12 and token only to their intended owner through a protected channel; send the P12 password separately. Do not hand administrator credentials to ordinary executors. The token stays in tab-scoped session storage. Refresh revalidates identity before restoring the page; sign-out or credential expiry clears the session. A browser may restore the session when restoring a closed tab.
"#,
            files = if private {
                "This directory contains:\n\n- `README.md`: these instructions, without keys, tokens or passwords.\n- `ca.crt`: public CA certificates.\n- `client.p12`: the selected user's certificate and private key; private credentials.\n- `token.txt`: the matching API bearer token; private credentials.\n\nThis is a private delivery package for its intended owner, not a public download. The P12 password is not stored here and must be delivered separately."
            } else {
                "This directory contains only `README.md` and the public CA bundle `ca.crt`. Obtain your personal `client.p12`, matching original token and P12 password separately from the administrator. Generating these instructions does not create or rotate accounts or credentials."
            }
        ),
    })
}
struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn publish(staging: &Path, destination: &Path) -> Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let from = CString::new(staging.as_os_str().as_bytes())?;
    let to = CString::new(destination.as_os_str().as_bytes())?;
    // SAFETY: Both paths are live NUL-terminated strings. NOREPLACE keeps even
    // raced empty directories and symlinks intact; no fallback weakens this rule.
    // Invoke the Linux syscall directly: the static musl runtime need not expose
    // the glibc renameat2 symbol.
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error())
            .context("refusing to replace GUI delivery directory");
    }
    Ok(())
}
pub(super) async fn run(args: Args) -> Result<()> {
    let destination = std::path::absolute(&args.output_dir)?;
    ensure!(
        fs::symlink_metadata(&destination).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "refusing to replace existing GUI delivery directory"
    );
    let config = args.connection.config.load()?;
    let (ca, certificates) = super::certificate_export::ca_bundle(&config)?;
    let (subject, fingerprint, expires) = certificate(&config)?;
    let me = converyn_runtime::interface::client::request(
        &config.clone().client()?,
        reqwest::Method::GET,
        "/api/v1/identity/me",
        None,
    )
    .await?;
    let name=me["identity"]["name"].as_str().context("GUI login instructions require a named person's --config/--context, not bootstrap, Node or peer credentials")?;
    let role = if me["capabilities"]["super_admin"] == true {
        "super_admin"
    } else {
        "See converyn auth status for effective workflow and platform permissions"
    };
    ensure!(
        !me["identity"]["disabled"].as_bool().unwrap_or(true)
            && me["credential"]["fingerprint"] == fingerprint,
        "selected local credential does not match the authenticated GUI identity"
    );

    let guide = document(
        &config,
        args.language,
        args.with_credentials,
        &LoginIdentity {
            subject: &subject,
            fingerprint: &fingerprint,
            expires,
            name,
            role,
        },
        &certificates,
    )?;
    let parent = destination.parent().context("GUI output parent missing")?;
    let staging = Staging(parent.join(format!(".converyn-gui-login-{}", pki::random_token()?)));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&staging.0)
        .context("create staging directory beside GUI output; its parent must exist")?;
    pki::private(&staging.0.join("ca.crt"), ca.as_bytes())?;
    pki::private(&staging.0.join("README.md"), guide.as_bytes())?;
    if args.with_credentials {
        let credentials = ContextConfig::from_config(config.clone())?;
        let bytes = super::certificate_export::current_pkcs12(config, args.password_file)?;
        pki::private(&staging.0.join("client.p12"), &bytes)?;
        pki::private(
            &staging.0.join("token.txt"),
            credentials.credentials.token.as_bytes(),
        )?;
    }
    fs::File::open(&staging.0)?.sync_all()?;
    publish(&staging.0, &destination)?;
    fs::File::open(parent)?.sync_all()?;
    output::print(
        &json!({"output_dir":destination,"guide":"README.md","ca":"ca.crt","contains_private_credentials":args.with_credentials,"certificate_subject":subject,"certificate_fingerprint":fingerprint,"identity":me["identity"],"files":if args.with_credentials {vec!["README.md","ca.crt","client.p12","token.txt"]}else{vec!["README.md","ca.crt"]}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guide_is_public_and_describes_actual_delivery_mode() {
        let config:Config=serde_json::from_value(json!({"cluster":"test","servers":["https://127.0.0.1:8443"],"token_file":"/private/do-not-print-token","tls":{"cert":"/private/cert","key":"/private/do-not-print-key","ca":"/private/ca"}})).unwrap();
        for lang in [Language::Chinese, Language::English] {
            for private in [false, true] {
                let text = document(
                    &config,
                    lang,
                    private,
                    &LoginIdentity {
                        subject: "user-1",
                        fingerprint: "abc",
                        expires: 2_000_000_000_000,
                        name: "Alice",
                        role: "executor",
                    },
                    &[json!({"sha256":"ca-sha","expires_ms":2_000_000_000_000u64})],
                )
                .unwrap();
                assert!(
                    text.contains("test")
                        && text.contains("https://127.0.0.1:8443")
                        && text.contains("ca-sha")
                );
                assert!(!text.contains("do-not-print"));
                assert_eq!(
                    text.contains("- `client.p12`: ") || text.contains("- `client.p12`："),
                    private
                );
            }
        }
    }
    #[test]
    fn publication_refuses_even_existing_empty_directories_and_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let staged = root.path().join("staged");
        fs::create_dir(&staged).unwrap();
        fs::write(staged.join("README.md"), "safe").unwrap();
        let out = root.path().join("out");
        fs::create_dir(&out).unwrap();
        assert!(publish(&staged, &out).is_err());
        assert!(staged.join("README.md").exists());
        fs::remove_dir(&out).unwrap();
        std::os::unix::fs::symlink(&staged, &out).unwrap();
        assert!(publish(&staged, &out).is_err());
        assert!(staged.join("README.md").exists());
    }
}
