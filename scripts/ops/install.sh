#!/usr/bin/env bash
# Fresh installation and explicit recovery use the existing Converyn CLI.
# Private bundles are durable inputs; this installer never reinitializes an existing instance.
set +x
set -Eeuo pipefail
umask 077

INSTALL_SCRIPT=$(readlink -f -- "${BASH_SOURCE[0]}")
MODE=deploy
BINARY='' BINARY_URL='' EXPECTED_SHA='' DIRECTORY='' CLUSTER='' OPERATOR=''
BUNDLE='' BOOTSTRAP='' HARDWARE_FILE='' SSH_KEY='' KNOWN_HOSTS='' SSH_USER='' SSH_PORT=22
SSH_AUTH=auto SSH_CONTROL_DIR=''
FIREWALL_MODE=auto SELINUX_MODE=keep
declare -A SSH_TARGETS=() SUDO_MODES=() SUDO_PASSWORDS=()
ASSUME_YES=false NONINTERACTIVE=false PREPARE_ONLY=false RESUME=false
MEMBERS=() CONTROLLERS=() AGENTS=()
STAGE='validate inputs'
FAILURE_CODE=1
declare -A SUPPLIED=()
TTY=false TTY_STATE='' SECRET_TTY_STATE=''
INPUT_CACHE='' INPUT_CACHE_ENABLED=true CACHE_READY=false CACHE_TEMP=''
BINARY_PROBE_DIR=''
AUTO_DEFAULT_CONTEXT=true CLI_STAGE='' CLI_CONTEXT_USER='' CLI_CONTEXT_BINARY=''
declare -A INPUT_DEFAULTS=() CLI_SET=()

log() { printf '[converyn-install] %s\n' "$*" >&2; }
fail() { log "$*"; exit "$FAILURE_CODE"; }
usage() {
    cat <<'USAGE'
Usage: bash install.sh [deploy|controller|agent|client] [options]

No arguments opens an interactive Linux/systemd installation wizard.

Binary input (required unless an existing/repository binary is selected):
  --binary FILE              Reviewed Linux Converyn binary, also used on remote hosts
  --binary-url HTTPS_URL     Download a raw binary over verified HTTPS
  --sha256 SHA256            Required with --binary-url; optional with --binary

Deployment:
  --cluster NAME             New instance name
  --installation DIR         Private installation record; create for deploy, reuse with --resume
  --member ID=IP:PORT         Repeat for 1, 3 or 5 distinct controllers
  --controller ID=TARGET     local or SSH user@host; defaults to local for one member
  --agent NODE=TARGET        Optional; repeat for local/SSH Agents
  --operator-name NAME       First named super administrator (required for deploy)
  --prepare-only             Generate a private bundle, without host installation or SSH
  --resume                   Resume the same installation plan; preserve original identity
  --input-cache FILE         Private non-secret wizard defaults (interactive only; deployment wizard)
  --no-input-cache           Do not read or write wizard input defaults
  --no-default-context       Skip importing the installed OP into the operator CLI config

SSH:
  --ssh-auth MODE            auto (default), key or password; passwords require a terminal
  --ssh-user USER            SSH user for derived controller targets
  --ssh-key FILE             Optional key selector (auto/key); existing SSH agent also works
  --ssh-known-hosts FILE      Host keys (default: ~/.ssh/known_hosts); interactive first-contact verification
  --ssh-port PORT            Global SSH port (default: 22)

Host preparation:
  --firewall auto|skip       Open each controller's actual TCP listen port (default: auto)
  --selinux keep|permissive  Keep SELinux policy, or explicitly relax this boot only

Local roles (SSH/topology and deployment-only options do not apply):
  controller --bundle DIR    Install one generated controller-N directory
  agent --bootstrap FILE    Enroll/install one Agent using its one-use bootstrap file
  --hardware-id-file FILE    Existing unique identity file on each Agent host (optional)

  --yes                     Accept the displayed installation plan
  --non-interactive         Reject missing inputs instead of prompting
  --help, -h                Show this help

Binary discovery: explicit --binary, PATH, adjacent release binary, repository build.
--installation also selects download staging for local roles with --binary-url.
Progress/errors go to stderr; stdout contains the final text result. Exit: 0 success/help, 2 input error, 1 execution failure/cancel.
Use --yes --non-interactive for unattended execution; --yes alone still permits prompts.

The script requires Bash 4.4+, GNU coreutils, flock and systemd on service hosts. Deploy verification also uses curl.
Interactive remote deployment supports SSH and sudo passwords, prompted separately for each target.
--non-interactive requires working SSH keys/agent and passwordless sudo. Passwords are never recorded.
It does not build Rust, install Python/jq, or reset instances. Controller firewall ports are configured unless --firewall skip is selected.
USAGE
}
need() { command -v "$1" >/dev/null 2>&1 || fail "Required command missing: $1"; }
identifier() { [[ $1 =~ ^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$ ]] || fail "Invalid identity: $1"; }
safe_path() {
    [[ $1 =~ ^/[A-Za-z0-9_./-]+$ && $1 != / && $1 != */../* && $1 != */./* && $1 != */.. && $1 != */. && $1 != *//* ]] || fail "Use a normalized absolute path without spaces: $1"
}
private_directory() {
    local directory=$1 owner mode parent
    safe_path "$directory"
    [[ ! -L $directory ]] || fail "Private directory must not be a symlink: $directory"
    parent=$directory
    while [[ $parent != / ]];do
        [[ ! -L $parent ]] || fail "Private directory ancestor is a symlink: $parent"
        if [[ -e $parent ]];then
            owner=$(stat -c '%u' -- "$parent");mode=$(stat -c '%a' -- "$parent")
            [[ $owner == "$EUID" || $owner == 0 ]] || fail "Private directory ancestor has another owner: $parent"
            (( (8#$mode & 0022)==0 || (owner==0 && (8#$mode & 01000)!=0) )) || fail "Unprotected private directory ancestor: $parent"
        fi
        parent=${parent%/*};[[ -n $parent ]] || parent=/
    done
    mkdir -p -- "$directory"
    owner=$(stat -c '%u' -- "$directory"); mode=$(stat -c '%a' -- "$directory")
    [[ $owner == "$EUID" && $mode == 700 ]] || fail "Private directory must be owned by the current user, mode 0700: $directory"
}
private_file() {
    local file=$1 owner mode
    [[ -f $file && ! -L $file ]] || fail "Expected a private regular file: $file"
    owner=$(stat -c '%u' -- "$file");mode=$(stat -c '%a' -- "$file")
    [[ ( $owner == "$EUID" || ( $EUID == 0 && $owner == "${SUDO_UID:-0}" ) ) && $mode == 600 ]] || fail "Private input must be owned by the current/invoking sudo user, mode 0600: $file"
}
file_sha() { sha256sum -- "$1" | cut -d ' ' -f 1; }
mark() { printf '%s\n' "$2" > "$1.new"; chmod 600 "$1.new"; mv -f -- "$1.new" "$1"; }
ask() {
    local variable=$1 label=$2 default=${3:-} value
    [[ $TTY == true && $NONINTERACTIVE == false ]] || fail "Missing input: $label"
    # Readline handles both terminal Backspace encodings and cursor editing.
    bind 'set editing-mode emacs' 2>/dev/null || :
    bind '"\C-h": backward-delete-char' 2>/dev/null || :
    bind '"\C-?": backward-delete-char' 2>/dev/null || :
    if ! IFS= read -r -e -i "$default" -p "$label: " value <&3 2>&3;then fail 'Interactive input closed';fi
    value=${value#"${value%%[![:space:]]*}"};value=${value%"${value##*[![:space:]]}"}
    printf -v "$variable" '%s' "$value"
}
valid_path() {
    [[ $1 =~ ^/[A-Za-z0-9_./-]+$ && $1 != / && $1 != */../* && $1 != */./* && $1 != */.. && $1 != */. && $1 != *//* ]]
}
valid_identifier() {
    [[ $1 =~ ^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$ ]] && return 0
    log '请输入 1–64 位名称，只使用字母、数字、点、下划线或短横线 / Invalid identity';return 1
}
valid_ipv4() {
    local ip=$1 octet
    [[ $ip =~ ^(0|[1-9][0-9]{0,2})\.(0|[1-9][0-9]{0,2})\.(0|[1-9][0-9]{0,2})\.(0|[1-9][0-9]{0,2})$ ]] || return 1
    local -a parts;IFS=. read -r -a parts <<< "$ip"
    for octet in "${parts[@]}";do ((10#$octet<=255)) || return 1;done
}
canonical_ip() {
    local ip=${1,,} prefix suffix part missing
    local -a left=() right=()
    if valid_ipv4 "$ip";then printf '%s' "$ip";return 0;fi
    if [[ $ip == *.* ]];then
        valid_ipv4 "${ip##*:}" || return 1
        local -a octets;IFS=. read -r -a octets <<< "${ip##*:}"
        printf -v suffix '%x:%x' "$((10#${octets[0]}*256+10#${octets[1]}))" "$((10#${octets[2]}*256+10#${octets[3]}))"
        ip=${ip%:*}:$suffix
    fi
    [[ $ip =~ ^[a-f0-9:]+$ && $ip == *:* ]] || return 1
    if [[ $ip == *::* ]];then
        prefix=${ip%%::*};suffix=${ip#*::}
        [[ $prefix != *::* && $suffix != *::* && $prefix != *: && $suffix != :* ]] || return 1
        [[ -z $prefix ]] || IFS=: read -r -a left <<< "$prefix"
        [[ -z $suffix ]] || IFS=: read -r -a right <<< "$suffix"
        missing=$((8-${#left[@]}-${#right[@]}));((missing>0)) || return 1
    else
        [[ $ip != :* && $ip != *: ]] || return 1
        IFS=: read -r -a left <<< "$ip";[[ ${#left[@]} == 8 ]] || return 1;missing=0
    fi
    for part in "${left[@]}" "${right[@]}";do [[ $part =~ ^[a-f0-9]{1,4}$ ]] || return 1;done
    for part in "${left[@]}";do printf '%04x:' "$((16#$part))";done
    while ((missing>0));do printf '0000:';missing=$((missing-1));done
    for part in "${right[@]}";do printf '%04x:' "$((16#$part))";done
}
valid_ip() {
    canonical_ip "$1" >/dev/null && return 0
    log '固定 IP 必填，请输入有效 IPv4 或 IPv6 地址，不要填 SSH 用户或端口 / A valid controller IP is required';return 1
}
valid_port() {
    [[ $1 =~ ^[1-9][0-9]{0,4}$ ]] && ((10#$1<=65535)) && return 0
    log '端口必须是 1–65535 / Port must be 1–65535';return 1
}
valid_endpoint() {
    local endpoint=$1 ip port
    if [[ $endpoint == \[*\]:* ]];then ip=${endpoint#\[};ip=${ip%\]:*};port=${endpoint##*:}
    else [[ $endpoint != *:*:* && $endpoint == *:* ]] || return 1;ip=${endpoint%:*};port=${endpoint##*:};fi
    canonical_ip "$ip" >/dev/null && [[ $port =~ ^[1-9][0-9]{0,4}$ ]] && ((10#$port<=65535))
}
valid_target() {
    local target=$1 user host
    [[ $target == local ]] && return 0
    [[ $target == *@* ]] || return 1
    user=${target%%@*};host=${target#*@};host=${host#\[};host=${host%\]}
    [[ $user =~ ^[A-Za-z_][A-Za-z0-9_-]*$ && -n $host && $host != *..* ]] || return 1
    if [[ $host == *:* ]];then canonical_ip "$host" >/dev/null
    elif [[ $host =~ ^[0-9.]+$ ]];then valid_ipv4 "$host"
    else [[ $host =~ ^[A-Za-z0-9][A-Za-z0-9_.-]*$ ]];fi
}
normalize_target() {
    local value=$1 user host
    if [[ $value != local && $value != *@* && -n $value ]];then value=$SSH_USER@$value;fi
    if [[ $value != local && $value == *@* ]];then
        user=${value%%@*};host=${value#*@};host=${host#\[};host=${host%\]}
        if [[ $host == *:* ]];then value=$user@[$host];fi
    fi
    printf '%s' "$value"
}
target_host() {
    local host=${1#*@};host=${host#\[};host=${host%\]}
    if [[ $host == *:* ]];then canonical_ip "$host";else printf '%s' "${host,,}";fi
}
expand_input_path() {
    case $1 in '~') printf '%s' "$HOME";; '~/'*) printf '%s/%s' "$HOME" "${1#\~/}";; *) printf '%s' "$1";;esac
}
valid_operator() {
    [[ -n $1 && ${#1} -le 128 && $1 != -* && $1 != *$'\n'* && $1 != *$'\r'* && $1 != *$'\t'* ]] && return 0
    log '管理员显示名称必填，最多 128 字符 / Invalid operator display name';return 1
}
valid_mode() { [[ $1 =~ ^[1-5]$ ]] && return 0;log '请选择 1–5 / Choose 1–5';return 1; }
valid_controller_count() { [[ $1 == 3 || $1 == 5 ]] && return 0;log 'HA 控制器数量必须为 3 或 5 / Choose 3 or 5 controllers';return 1; }
valid_agent_count() { [[ $1 =~ ^(0|[1-9][0-9]{0,2})$ ]] && ((10#$1<=300)) && return 0;log 'Agent 数量必须为 0–300 / Choose 0–300 Agents';return 1; }
valid_ssh_user() { [[ $1 =~ ^[A-Za-z_][A-Za-z0-9_-]*$ ]] && return 0;log '请输入有效 SSH 用户名 / Invalid SSH username';return 1; }
valid_ssh_auth() { [[ $1 == auto || $1 == key || $1 == password ]] && return 0;log 'SSH 认证请选择 auto、key 或 password / Invalid SSH authentication mode';return 1; }
wizard_ssh_auth() {
    valid_ssh_auth "$1" || return 1
    if [[ $1 == password && -n $SSH_KEY && ${CLI_SET[SSH_KEY]+present} ]];then log '已指定 --ssh-key，请选 auto/key；使用 password 请去掉该参数 / Conflicting SSH key selector';return 1;fi
}
valid_file() { [[ -f $1 && -r $1 ]] && return 0;log '文件不存在或不可读，请修改路径 / File must exist and be readable';return 1; }
valid_known_hosts() {
    local file=$1 parent resolved line first second third fourth key_type key_data
    if [[ -z $file || $file == *[[:space:]]* ]];then log '请输入主机指纹文件路径（不含空格） / Enter a known_hosts file path without spaces';return 1;fi
    if [[ -e $file || -L $file ]];then
        if [[ ! -f $file || ! -r $file || ! -w $file ]];then log '主机指纹文件需要可读写 / known_hosts must be a readable, writable regular file';return 1;fi
        resolved=$(readlink -f -- "$file") || return 1
        case ${resolved##*/} in authorized_keys|id_rsa|id_ed25519|id_ecdsa|config|*.pem|*.key) log '请选择专用 known_hosts 文件，不要覆盖密钥或 SSH 配置 / Select a dedicated host-key file';return 1;;esac
        if grep -qE -- '^-----BEGIN (OPENSSH |RSA |EC |DSA |ENCRYPTED )?PRIVATE KEY-----' "$file";then log '请选择主机指纹文件，不要使用私钥文件 / This file contains a private key';return 1;fi
        if (( $(stat -c '%s' -- "$file") > 16777216 ));then log '主机指纹文件过大，请选择专用 known_hosts / Host-key file exceeds 16 MiB';return 1;fi
        while IFS= read -r line || [[ -n $line ]];do
            line=${line#"${line%%[![:space:]]*}"}
            [[ -n $line && $line != \#* ]] || continue
            first='' second='' third='' fourth=''
            read -r first second third fourth _ <<< "$line"
            if [[ $first == @cert-authority || $first == @revoked ]];then key_type=$third;key_data=$fourth
            else key_type=$second;key_data=$third;fi
            if [[ ! $key_type =~ ^(ssh-|ecdsa-|sk-|rsa-sha2-) || ! $key_data =~ ^[A-Za-z0-9+/=]+$ ]];then
                log '文件不是有效的 known_hosts 格式，请选择专用主机指纹文件 / Select a valid known_hosts file';return 1
            fi
        done < "$file"
        return 0
    fi
    if [[ $file == */* ]];then parent=${file%/*};[[ -n $parent ]] || parent=/;else parent=.;fi
    while [[ ! -e $parent ]];do
        if [[ $parent != */* ]];then parent=.;else parent=${parent%/*};[[ -n $parent ]] || parent=/;fi
    done
    if [[ -d $parent && -w $parent && -x $parent ]];then return 0;fi
    log '主机指纹文件所在目录不可写 / The known_hosts parent directory is not writable';return 1
}
prepare_known_hosts() {
    [[ -n $KNOWN_HOSTS ]] || fail '--ssh-known-hosts must select an SSH host-key file'
    if [[ $TTY != true || $NONINTERACTIVE == true ]];then
        [[ -f $KNOWN_HOSTS && -r $KNOWN_HOSTS ]] || fail 'Noninteractive SSH needs an existing known_hosts file with verified host keys'
        return
    fi
    valid_known_hosts "$KNOWN_HOSTS" || fail 'Choose a writable known_hosts file for interactive first-contact confirmation'
    if [[ ! -e $KNOWN_HOSTS ]];then
        local parent destination_parent owner uid gid
        local -a missing=()
        parent=${KNOWN_HOSTS%/*};[[ $parent != "$KNOWN_HOSTS" ]] || parent=.;[[ -n $parent ]] || parent=/
        destination_parent=$parent
        while [[ ! -e $parent ]];do
            missing+=("$parent")
            if [[ $parent != */* ]];then parent=.;else parent=${parent%/*};[[ -n $parent ]] || parent=/;fi
        done
        owner=$(stat -c '%u' -- "$parent")
        mkdir -p -m 700 -- "$destination_parent"
        if [[ $EUID == 0 && ${SUDO_UID:-0} =~ ^[0-9]+$ && ${SUDO_GID:-0} =~ ^[0-9]+$ && ${SUDO_UID:-0} != 0 && $owner == "$SUDO_UID" ]];then
            uid=$SUDO_UID;gid=$SUDO_GID
            for parent in "${missing[@]}";do chown "$uid:$gid" -- "$parent";done
        fi
        (set -o noclobber; : > "$KNOWN_HOSTS") || fail 'Could not create known_hosts without overwriting an existing file'
        chmod 600 "$KNOWN_HOSTS"
        if [[ -n ${uid:-} ]];then chown "$uid:$gid" -- "$KNOWN_HOSTS";fi
    fi
}
valid_optional_key() { [[ -z $1 ]] || valid_file "$1"; }
valid_bootstrap() { (private_file "$1"); }
valid_binary_source() { [[ $1 == 1 || $1 == 2 ]] && return 0;log '请选择 1=本地文件 或 2=HTTPS 下载 / Choose 1 or 2';return 1; }
valid_binary_input() { (verify_binary "$1"); }
valid_https_url() { [[ $1 == https://?* && $1 != *[[:space:]]* ]] && return 0;log '请输入经过审核的 HTTPS 下载地址 / Use a reviewed HTTPS URL';return 1; }
valid_sha() { [[ $1 =~ ^[a-fA-F0-9]{64}$ ]] && return 0;log 'SHA-256 必须是 64 位十六进制摘要 / Invalid SHA-256';return 1; }
default_install_directory() {
    [[ ${HOME:-} == /* ]] || fail 'Set an absolute HOME or specify --installation'
    printf '%s/.local/share/converyn-install/%s' "${HOME%/}" "$CLUSTER"
}
valid_install_directory() {
    local path=$1 parent owner mode
    if ! valid_path "$path";then log '请使用规范的绝对路径（不含空格或 ..） / Use a normalized absolute directory';return 1;fi
    parent=$path
    while [[ $parent != / ]];do
        if [[ -L $parent ]];then log '安装目录及其父目录不能是符号链接 / Installation path contains a symlink';return 1;fi
        if [[ -e $parent ]];then
            owner=$(stat -c '%u' -- "$parent");mode=$(stat -c '%a' -- "$parent")
            if [[ $owner != "$EUID" && $owner != 0 ]] || ! (( (8#$mode & 0022)==0 || (owner==0 && (8#$mode & 01000)!=0) ));then log '安装目录的父目录所有权或权限不安全 / Unprotected installation path';return 1;fi
        fi
        parent=${parent%/*};[[ -n $parent ]] || parent=/
    done
    if [[ -e $path ]];then
        if [[ $RESUME != true ]];then log '安装目录已存在：请选择新目录；恢复原安装请加 --resume / Existing installation requires --resume';return 1;fi
        if [[ ! -d $path || $(stat -c '%u %a' -- "$path") != "$EUID 700" ]];then log '恢复目录须属于当前用户且权限为 0700 / Invalid private installation directory';return 1;fi
    elif [[ $RESUME == true ]];then log '恢复目录不存在，请选择原安装记录目录 / Resume directory does not exist';return 1
    fi
    parent=${path%/*};[[ -n $parent ]] || parent=/
    while [[ ! -e $parent ]];do parent=${parent%/*};[[ -n $parent ]] || parent=/;done
    if [[ ! -d $parent || ! -w $parent || ! -x $parent ]];then log "当前用户不能在 $parent 创建安装目录，请选用自己的 home / Directory is not writable";return 1;fi
}
wizard_controller_ip() {
    local key
    valid_ip "$1" || return 1;key=$(canonical_ip "$1")
    if [[ ${wizard_ips[$key]+present} ]];then log "该 IP 已用于 Controller ${wizard_ips[$key]}，请换一个地址 / Duplicate controller IP: $1";return 1;fi
}
wizard_controller_target() {
    local host
    valid_target "$1" || { log '请输入 local 或 SSH用户@主机；也可直接输入 IP / Invalid controller target';return 1; }
    host=$(target_host "$1")
    if [[ ${wizard_controller_hosts[$host]+present} ]];then log "该主机已用于 Controller ${wizard_controller_hosts[$host]} / Duplicate controller installation host: $1";return 1;fi
    if [[ $1 == local && $EUID != 0 ]];then log '本机安装请用 sudo 启动脚本；或填写普通用户@本机IP，通过 SSH 提权 / Local installation needs sudo';return 1;fi
}
wizard_agent_uid() {
    valid_identifier "$1" || return 1
    if [[ ${wizard_agent_ids[$1]+present} ]];then log "Agent UID 已使用，请换一个名称 / Duplicate Agent UID: $1";return 1;fi
}
wizard_agent_target() {
    local host
    valid_target "$1" || { log '请输入 local、用户@主机或 IP / Invalid Agent target';return 1; }
    host=$(target_host "$1")
    if [[ ${wizard_agent_hosts[$host]+present} ]];then log "该主机已用于 Agent ${wizard_agent_hosts[$host]}，请为另一个 Agent 选择其他机器 / Duplicate Agent host: $1";return 1;fi
    if [[ $1 == local && $EUID != 0 ]];then log '本机安装请用 sudo 启动脚本，或填写用户@本机IP / Local installation needs sudo';return 1;fi
}
cache_key_allowed() {
    case $1 in mode|cluster|operator|directory|controller_count|agent_count|local_agent_uid|ssh_user|ssh_auth|known_hosts|ssh_key|binary_source|binary|sha256|bootstrap) return 0;;esac
    [[ $1 =~ ^controller_[1-5]_(ip|port|target)$ || $1 =~ ^agent_([1-9][0-9]{0,2})_(uid|target)$ ]]
}
cache_write() {
    [[ $CACHE_READY == true ]] || return 0
    local key
    CACHE_TEMP=$(mktemp "${INPUT_CACHE%/*}/.installer-inputs.XXXXXXXX")
    { printf '# Converyn installer input defaults; data only, never shell code\n'
      for key in "${!INPUT_DEFAULTS[@]}";do printf '%s\t%s\n' "$key" "${INPUT_DEFAULTS[$key]}";done
    } > "$CACHE_TEMP"
    chmod 600 "$CACHE_TEMP";mv -f -- "$CACHE_TEMP" "$INPUT_CACHE";CACHE_TEMP=''
}
cache_set() {
    [[ $CACHE_READY == true ]] || return 0
    cache_key_allowed "$1" || return 0
    [[ $2 != *$'\t'* && $2 != *$'\n'* && $2 != *$'\r'* ]] || return 0
    if [[ $1 == cluster && ${INPUT_DEFAULTS[cluster]+present} && ${INPUT_DEFAULTS[cluster]} != "$2" && ! ${CLI_SET[DIRECTORY]+present} ]];then unset 'INPUT_DEFAULTS[directory]';fi
    INPUT_DEFAULTS[$1]=$2;cache_write
}
cache_init() {
    [[ $INPUT_CACHE_ENABLED == true && $TTY == true && $NONINTERACTIVE == false ]] || return 0
    local parent ancestor owner mode line key value header
    INPUT_CACHE=${INPUT_CACHE:-${XDG_STATE_HOME:-$HOME/.local/state}/converyn/installer/inputs.tsv}
    valid_path "$INPUT_CACHE" || fail '--input-cache requires a normalized absolute file path'
    parent=${INPUT_CACHE%/*};ancestor=$parent
    while [[ $ancestor != / ]];do
        [[ ! -L $ancestor ]] || fail 'Input cache parent must not be a symlink'
        if [[ -e $ancestor ]];then
            owner=$(stat -c '%u' -- "$ancestor");mode=$(stat -c '%a' -- "$ancestor")
            [[ $owner == "$EUID" || $owner == 0 ]] || fail 'Input cache parent has another owner'
            (( (8#$mode & 0022)==0 || (owner==0 && (8#$mode & 01000)!=0) )) || fail 'Input cache parent is writable by other users'
        fi
        ancestor=${ancestor%/*};[[ -n $ancestor ]] || ancestor=/
    done
    mkdir -p -- "$parent"
    [[ $(stat -c '%u' -- "$parent") == "$EUID" ]] && (( (8#$(stat -c '%a' -- "$parent") & 0022)==0 )) || fail 'Use an input cache directory owned by the current user and not writable by others'
    if [[ -e $INPUT_CACHE || -L $INPUT_CACHE ]];then
        [[ -f $INPUT_CACHE && ! -L $INPUT_CACHE && $(stat -c '%u' -- "$INPUT_CACHE") == "$EUID" ]] || fail 'Input cache must be a regular file owned by the current user, not a symlink'
        [[ $(stat -c '%s' -- "$INPUT_CACHE") -le 1048576 ]] || fail 'Input cache exceeds 1 MiB'
        if [[ -s $INPUT_CACHE ]];then
            IFS= read -r header < "$INPUT_CACHE" || :
            [[ $header == '# Converyn installer input defaults; data only, never shell code' ]] || fail 'This file is not a Converyn input cache; select another --input-cache path'
        fi
        chmod 600 "$INPUT_CACHE"
        while IFS= read -r line || [[ -n $line ]];do
            [[ $line == *$'\t'* ]] || continue;key=${line%%$'\t'*};value=${line#*$'\t'}
            cache_key_allowed "$key" || continue
            [[ $value != *$'\t'* && $value != *$'\r'* ]] || continue
            INPUT_DEFAULTS[$key]=$value
        done < "$INPUT_CACHE"
        log "Loaded input defaults / 已载入输入缓存: $INPUT_CACHE"
    fi
    CACHE_READY=true
}
ask_option() {
    local variable=$1 label=$2 fallback=$3 key=$4 validator=$5 input default=$3
    if [[ ! ${CLI_SET[$variable]+present} && ${INPUT_DEFAULTS[$key]+present} ]];then default=${INPUT_DEFAULTS[$key]};fi
    while :;do
        ask input "$label" "$default"
        case $key in *_target) input=$(normalize_target "$input");; directory|known_hosts|ssh_key|binary|bootstrap) input=$(expand_input_path "$input");;esac
        if "$validator" "$input";then printf -v "$variable" '%s' "$input";cache_set "$key" "$input";return 0;fi
        log '请修改这一项后继续，前面的输入保留 / Correct this field to continue'
        default=$input
    done
}
confirm() {
    local answer
    [[ $ASSUME_YES == false ]] || return 0
    ask answer 'Proceed / 开始安装? (yes/no)' no
    [[ $answer == yes ]] || { log 'Installation canceled';exit 1; }
}
root_binary_host() {
    [[ $EUID == 0 ]] || fail 'Installing into /usr/local/bin requires root'
    [[ $(uname -s) == Linux ]] || fail 'Installed hosts must run Linux'
}
root_host() {
    [[ $EUID == 0 ]] || fail 'Local host installation requires root; use sudo bash install.sh ...'
    [[ $(uname -s) == Linux ]] || fail 'Installed hosts must run Linux'
    need systemctl; [[ -d /run/systemd/system ]] || fail 'A running systemd host is required'
}
architecture() {
    case $(uname -m) in x86_64) printf '3e00';; aarch64|arm64) printf 'b700';; *) fail 'Supported binary architectures: x86_64 and aarch64';; esac
}
verify_binary() {
    local path=$1 magic machine actual
    [[ -f $path && ! -L $path ]] || fail "Binary is not a regular file: $path"
    actual=$(file_sha "$path")
    [[ -z $EXPECTED_SHA || $actual == "$EXPECTED_SHA" ]] || fail 'Binary SHA-256 differs from the reviewed value'
    magic=$(od -An -tx1 -N6 -- "$path" | tr -d '[:space:]')
    machine=$(od -An -tx1 -j18 -N2 -- "$path" | tr -d '[:space:]')
    [[ $magic == 7f454c460201 && $machine == "$(architecture)" ]] || fail 'Binary must be a matching 64-bit little-endian Linux ELF'
    [[ -x $path ]] || fail "Binary is not executable: $path"
    "$path" --help >/dev/null
}
select_binary() {
    local candidate source_choice binary_arch script_directory
    EXPECTED_SHA=${EXPECTED_SHA,,}
    [[ -z $BINARY || -z $BINARY_URL ]] || fail 'Choose --binary or --binary-url'
    if [[ -n $BINARY_URL ]]; then
        if [[ -z $EXPECTED_SHA && $TTY == true && $NONINTERACTIVE == false ]];then ask_option EXPECTED_SHA 'Reviewed binary SHA-256' "$EXPECTED_SHA" sha256 valid_sha;EXPECTED_SHA=${EXPECTED_SHA,,};fi
        [[ $BINARY_URL == https://* && $EXPECTED_SHA =~ ^[a-fA-F0-9]{64}$ ]] || fail 'HTTPS download requires an independently reviewed --sha256'
        need curl
    elif [[ -z $BINARY ]]; then
        BINARY=$(command -v converyn || true)
        script_directory=${INSTALL_SCRIPT%/*}
        case $(uname -m) in x86_64) binary_arch=amd64;;aarch64|arm64) binary_arch=arm64;;*) binary_arch=unknown;;esac
        if [[ -z $BINARY ]];then
            for candidate in "$script_directory/converyn-linux-$binary_arch" "$script_directory/converyn" "$script_directory/../../target/release/converyn" "$script_directory/../../target/debug/converyn"; do
                if [[ -f $candidate && -x $candidate ]]; then BINARY=$candidate;break;fi
            done
        fi
        if [[ -z $BINARY ]]; then
            ask_option source_choice 'Binary source / 二进制来源: 1=本地文件 2=HTTPS下载' 1 binary_source valid_binary_source
            if [[ $source_choice == 2 ]];then
                ask_option BINARY_URL 'Reviewed raw binary HTTPS URL' "$BINARY_URL" binary_url valid_https_url
                ask_option EXPECTED_SHA 'Reviewed binary SHA-256' "$EXPECTED_SHA" sha256 valid_sha;EXPECTED_SHA=${EXPECTED_SHA,,}
                [[ $BINARY_URL == https://* && $EXPECTED_SHA =~ ^[a-f0-9]{64}$ ]] || fail 'HTTPS download requires a reviewed SHA-256'
                need curl
            else ask_option BINARY 'Reviewed Converyn binary path / 二进制路径' '' binary valid_binary_input;fi
        fi
    fi
    if [[ -n $BINARY ]]; then BINARY=$(readlink -f -- "$BINARY") || fail 'Binary path is missing or invalid';verify_binary "$BINARY";fi
    EXPECTED_SHA=${EXPECTED_SHA,,}
}
acquire_binary() {
    local destination=$1 before actual
    if [[ -f $destination ]]; then
        [[ $RESUME == true ]] || fail "Binary snapshot already exists: $destination"
        verify_binary "$destination"
        if [[ -n $BINARY ]]; then [[ $(file_sha "$destination") == "$(file_sha "$BINARY")" ]] || fail 'Resume binary differs from the original snapshot';fi
    elif [[ -n $BINARY_URL ]]; then
        curl -q --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 \
            --connect-timeout 15 --max-time 900 --output "$destination.download" "$BINARY_URL"
        actual=$(file_sha "$destination.download")
        [[ $actual == "$EXPECTED_SHA" ]] || fail 'Downloaded binary SHA-256 mismatch; not executed'
        chmod 700 "$destination.download";mv -- "$destination.download" "$destination"
        verify_binary "$destination"
    else
        before=$(file_sha "$BINARY");cp --reflink=auto -- "$BINARY" "$destination";chmod 700 "$destination"
        [[ $(file_sha "$destination") == "$before" ]] || fail 'Binary changed during staging; not executed'
        verify_binary "$destination"
    fi
    BINARY=$destination
}
validate_binary_contract() {
    local profile expected command stage_status=0
    local -a words
    BINARY_PROBE_DIR=$(mktemp -d /tmp/converyn-binary-probe.XXXXXXXX);chmod 700 "$BINARY_PROBE_DIR"
    # Downloaded binaries are staged and checked before an installation record is created.
    if [[ -z $BINARY ]];then acquire_binary "$BINARY_PROBE_DIR/converyn";BINARY_URL='';fi
    # Reject a binary with a different operator CLI before creating bundles or touching hosts.
    for command in 'create identities' 'resume credentials' 'get members';do
        read -r -a words <<< "$command"
        if ! "$BINARY" "${words[@]}" --help > /dev/null 2> "$BINARY_PROBE_DIR/probe-error";then
            fail "Converyn binary is incompatible with this installer: missing '$command'. Use the installer and binary from the same release. SHA-256: $(file_sha "$BINARY")"
        fi
    done
    profile=$BINARY_PROBE_DIR/probe/controller-1;expected=$BINARY_PROBE_DIR/probe/output-keys.json
    "$BINARY" admin init --directory "$BINARY_PROBE_DIR/probe" --cluster installer-probe --member 1=127.0.0.1:8443 > /dev/null 2> "$BINARY_PROBE_DIR/probe-error" || stage_status=$?
    if ((stage_status)) || [[ ! -f $expected || ! -f $profile/server.json || ! -f $profile/output-keys.json ]] || ! cmp -s -- "$expected" "$profile/output-keys.json" || ! grep -qE '"output_keyring"[[:space:]]*:[[:space:]]*"output-keys\.json"' "$profile/server.json";then
        fail "Converyn binary is incompatible with this installer: admin init did not create the required shared output-key ring and controller profile. Use the matching current release binary; version number alone is insufficient. SHA-256: $(file_sha "$BINARY")"
    fi
}
cleanup_binary_probe() {
    if [[ $BINARY_PROBE_DIR == /tmp/converyn-binary-probe.* && -d $BINARY_PROBE_DIR && ! -L $BINARY_PROBE_DIR ]];then rm -rf -- "$BINARY_PROBE_DIR";fi
    BINARY_PROBE_DIR=''
}
validate_controller_profile() {
    local directory=$1
    [[ -d $directory && ! -L $directory ]] || fail "Controller bundle directory is invalid: $directory"
    [[ -f $directory/server.json && ! -L $directory/server.json ]] || fail "Controller bundle lacks server.json: $directory"
    [[ -f $directory/output-keys.json && ! -L $directory/output-keys.json ]] || fail "Controller bundle lacks output-keys.json: $directory. Use the matching current installer and binary; keep the original installation record."
    grep -qE '"output_keyring"[[:space:]]*:[[:space:]]*"output-keys\.json"' "$directory/server.json" || fail "Controller profile does not reference its output keyring: $directory/server.json"
}
validate_prepared_bundle() {
    local bundle=$1 value id
    [[ -f $bundle/output-keys.json && ! -L $bundle/output-keys.json ]] || fail "Prepared bundle lacks its shared output keyring: $bundle. Keep the original record and use a matching installer/binary."
    for value in "${MEMBERS[@]}";do
        id=${value%%=*};validate_controller_profile "$bundle/controller-$id"
        cmp -s -- "$bundle/output-keys.json" "$bundle/controller-$id/output-keys.json" || fail "Controller $id has a different output keyring; refusing to initialize or overwrite identity"
    done
}
controller_listen_endpoint() {
    local line endpoint=''
    while IFS= read -r line || [[ -n $line ]];do
        if [[ $line =~ \"listen\"[[:space:]]*:[[:space:]]*\"([^\"]+)\" ]];then endpoint=${BASH_REMATCH[1]};break;fi
    done < "$BUNDLE/server.json"
    valid_endpoint "$endpoint" || fail 'Controller profile has an invalid listen address'
    printf '%s' "$endpoint"
}
configure_selinux() {
    local status=unavailable
    if command -v getenforce >/dev/null 2>&1;then status=$(getenforce 2>/dev/null || printf unknown);fi
    if [[ $SELINUX_MODE == keep ]];then log "SELinux: $status (kept)";return;fi
    case $status in
        Enforcing) need setenforce;setenforce 0;log 'SELinux is permissive for this boot; persistent policy was not changed';;
        Permissive|Disabled|unavailable) log "SELinux: $status; no enforcing policy to relax";;
        *) fail 'Cannot determine SELinux state; no SELinux policy was changed';;
    esac
}
configure_controller_firewall() {
    local endpoint host port key interface='' index device family cidr rest zone
    endpoint=$(controller_listen_endpoint);host=$(member_ip "1=$endpoint");port=${endpoint##*:}
    if [[ $FIREWALL_MODE == skip ]];then log "Controller firewall configuration skipped: TCP $port";return;fi
    key=$(canonical_ip "$host")
    if [[ $host == 127.* || $key == 0000:0000:0000:0000:0000:0000:0000:0001: ]];then log 'Loopback-only controller: no inbound firewall port is needed';return;fi
    if command -v firewall-cmd >/dev/null 2>&1 && firewall-cmd --state >/dev/null 2>&1;then
        need ip
        while read -r index device family cidr rest;do
            [[ $family == inet || $family == inet6 ]] || continue
            if [[ $(canonical_ip "${cidr%/*}" 2>/dev/null || :) == "$key" ]];then interface=${device%%@*};break;fi
        done < <(ip -o address show)
        [[ -n $interface ]] || fail "No local interface owns controller address $host; firewall was not changed"
        zone=$(firewall-cmd --get-zone-of-interface="$interface" 2>/dev/null || :)
        if [[ -z $zone || $zone == 'no zone' ]];then zone=$(firewall-cmd --get-default-zone);fi
        [[ -n $zone ]] || fail 'Cannot determine firewalld zone for the controller interface'
        firewall-cmd --zone="$zone" --add-port="$port/tcp" >/dev/null
        firewall-cmd --permanent --zone="$zone" --add-port="$port/tcp" >/dev/null
        log "Opened controller TCP $port in firewalld zone $zone (runtime and permanent)"
    elif command -v ufw >/dev/null 2>&1 && [[ $(LC_ALL=C ufw status 2>/dev/null || :) == 'Status: active'* ]];then
        ufw allow "$port/tcp" >/dev/null
        log "Opened controller TCP $port in UFW"
    else
        log "No active firewalld/UFW manager detected; controller TCP $port will be verified by connectivity checks"
    fi
}
install_binary() {
    local destination=/usr/local/bin/converyn hash old
    need flock;private_directory /run/lock/converyn-install
    exec 7>/run/lock/converyn-install/binary.lock;flock -n 7 || fail 'Another host binary installation is running'
    [[ ! -L $destination && ( ! -e $destination || -f $destination ) ]] || fail 'Installed binary path must be a regular file'
    hash=$(file_sha "$BINARY")
    if [[ -f $destination ]]; then
        old=$(file_sha "$destination")
        [[ $old != "$hash" ]] || return 0
        [[ ! -e /etc/converyn/agent.json && ! -e /var/lib/converyn-controller/server.json ]] || fail 'An existing instance uses a different build; follow the upgrade procedure'
        cp --preserve=mode -- "$destination" "$destination.before-$old"
    fi
    install -d -m 0755 /usr/local/bin
    install -m 0755 -- "$BINARY" /usr/local/bin/.converyn-install.new
    mv -f -- /usr/local/bin/.converyn-install.new "$destination"
}
controller_unit() {
    cat <<'UNIT'
[Unit]
Description=Converyn controller
Wants=network-online.target
After=network-online.target
[Service]
User=converyn-controller
Group=converyn-controller
WorkingDirectory=/var/lib/converyn-controller
ExecStart=/usr/local/bin/converyn server --config /var/lib/converyn-controller/server.json
Restart=on-failure
RestartSec=2
UMask=0077
[Install]
WantedBy=multi-user.target
UNIT
}
bundle_sha() {
    local directory=$1
    (cd "$directory"; find . -type f ! -path './data/*' -print0 | sort -z | xargs -0 sha256sum) | sha256sum | cut -d ' ' -f 1
}
install_controller() {
    local destination=/var/lib/converyn-controller receipt=/var/lib/converyn-controller/install.bundle.sha256
    local unit=/etc/systemd/system/converyn-controller.service expected account uid gid shell path
    root_host;need useradd;need getent;need flock
    private_directory /run/lock/converyn-install
    exec 8>/run/lock/converyn-install/controller.lock;flock -n 8 || fail 'Another controller installation is running'
    [[ -n $BUNDLE ]] || fail '--bundle is required for the controller role'
    validate_controller_profile "$BUNDLE"
    [[ -z $(find "$BUNDLE" -type l -print -quit) ]] || fail 'Controller bundle contains symlinks'
    expected=$(bundle_sha "$BUNDLE")
    if [[ -e $destination ]]; then
        [[ $RESUME == true && -f $receipt && ! -L $destination && $(cat "$receipt") == "$expected" ]] || fail 'Existing controller directory is not this installation; preserving it'
        while IFS= read -r -d '' path; do cmp -s -- "$path" "$destination/${path#"$BUNDLE"/}" || fail "Controller input changed: ${path#"$BUNDLE"/}";done < <(find "$BUNDLE" -type f -print0)
    elif [[ -e $unit ]]; then fail 'Existing controller service requires explicit migration';fi
    configure_selinux
    configure_controller_firewall
    if account=$(getent passwd converyn-controller); then
        IFS=: read -r _ _ uid gid _ _ shell <<< "$account"
        [[ $uid -gt 0 && ( $shell == */nologin || $shell == */false ) ]] || fail 'Existing converyn-controller account is not a service identity'
        [[ $(getent group converyn-controller | cut -d : -f 3) == "$gid" ]] || fail 'Existing controller account/group do not match'
    else
        shell=$(command -v nologin || true);[[ -n $shell ]] || shell=/usr/sbin/nologin
        useradd --system --user-group --no-create-home --shell "$shell" converyn-controller
    fi
    install_binary
    if [[ ! -e $destination ]]; then
        install -d -m 0700 "$destination"
        cp -a -- "$BUNDLE/." "$destination/"
        mark "$receipt" "$expected"
        chown -R converyn-controller:converyn-controller "$destination"
    fi
    if [[ -e $unit ]]; then cmp -s <(controller_unit) "$unit" || fail 'Existing controller unit differs; not overwritten'
    else controller_unit > "$unit";chmod 644 "$unit";fi
    systemctl daemon-reload
    systemctl enable --now converyn-controller
    systemctl is-active --quiet converyn-controller
    log 'Controller service started; quorum is checked after all members are installed.'
}
install_agent() {
    local marker=/var/lib/converyn/shell-install.bootstrap.sha256 expected
    root_host
    [[ -n $BOOTSTRAP ]] || fail '--bootstrap is required for the Agent role'
    private_file "$BOOTSTRAP"
    expected=$(file_sha "$BOOTSTRAP")
    if [[ -e /etc/converyn/agent.json ]]; then
        [[ $RESUME == true && -f $marker && $(cat "$marker") == "$expected" ]] || fail 'Existing Agent belongs to another installation; identity and state retained'
    fi
    configure_selinux
    install_binary
    install -d -m 0711 /var/lib/converyn
    mark "$marker" "$expected"
    local arguments=(admin install-local --bootstrap "$BOOTSTRAP")
    [[ -z $HARDWARE_FILE ]] || arguments+=(--hardware-id-file "$HARDWARE_FILE")
    /usr/local/bin/converyn "${arguments[@]}"
    systemctl is-active --quiet converyn-agent
    log 'Agent installation returned after its authenticated heartbeat was confirmed.'
}
ssh_arguments() {
    local batch=yes host_check=yes
    if [[ $TTY == true && $NONINTERACTIVE == false ]];then batch=no;host_check=ask;fi
    [[ $SSH_AUTH != password || $batch == no ]] || fail 'Password SSH requires an interactive terminal; omit --non-interactive or use --ssh-auth key'
    # Keep sockets separate from durable installation records. Only this invocation
    # can access them; subsequent ssh/scp calls reuse the authenticated connection.
    SSH_CONTROL_DIR=$(mktemp -d /tmp/converyn-ssh.XXXXXXXX)
    chmod 700 "$SSH_CONTROL_DIR"
    SSH_ARGS=(-p "$SSH_PORT" -o "BatchMode=$batch" -o "StrictHostKeyChecking=$host_check" -o "UserKnownHostsFile=$KNOWN_HOSTS" -o ConnectTimeout=10 -o ServerAliveInterval=10 -o ServerAliveCountMax=3 -o ControlMaster=auto -o "ControlPath=$SSH_CONTROL_DIR/%C" -o ControlPersist=600)
    SCP_ARGS=(-P "$SSH_PORT" -o "BatchMode=$batch" -o "StrictHostKeyChecking=$host_check" -o "UserKnownHostsFile=$KNOWN_HOSTS" -o ConnectTimeout=10 -o ServerAliveInterval=10 -o ServerAliveCountMax=3 -o ControlMaster=auto -o "ControlPath=$SSH_CONTROL_DIR/%C" -o ControlPersist=600)
    if [[ $SSH_AUTH == password ]];then
        SSH_ARGS+=(-o PubkeyAuthentication=no -o PreferredAuthentications=password,keyboard-interactive)
        SCP_ARGS+=(-o PubkeyAuthentication=no -o PreferredAuthentications=password,keyboard-interactive)
    fi
    if [[ $SSH_AUTH == key ]];then
        SSH_ARGS+=(-o PubkeyAuthentication=yes -o PreferredAuthentications=publickey)
        SCP_ARGS+=(-o PubkeyAuthentication=yes -o PreferredAuthentications=publickey)
    fi
    if [[ -n $SSH_KEY ]];then SSH_ARGS+=(-i "$SSH_KEY" -o IdentitiesOnly=yes);SCP_ARGS+=(-i "$SSH_KEY" -o IdentitiesOnly=yes);fi
}
ssh_cleanup() {
    local target
    if [[ -n $SSH_CONTROL_DIR ]];then
        for target in "${!SSH_TARGETS[@]}";do
            ssh -o BatchMode=yes "${SSH_ARGS[@]}" -O exit "$target" >/dev/null 2>&1 || :
        done
        if [[ $SSH_CONTROL_DIR == /tmp/converyn-ssh.* && -d $SSH_CONTROL_DIR && ! -L $SSH_CONTROL_DIR ]];then rm -rf -- "$SSH_CONTROL_DIR";fi
        SSH_CONTROL_DIR=''
    fi
    SUDO_PASSWORDS=()
    SUDO_MODES=()
    SSH_TARGETS=()
}
installer_cleanup() {
    local result=$?
    set +x
    trap - EXIT HUP INT TERM
    ssh_cleanup
    cleanup_binary_probe
    cleanup_cli_stage
    if [[ -n $CACHE_TEMP && -f $CACHE_TEMP && ! -L $CACHE_TEMP ]];then rm -f -- "$CACHE_TEMP";fi
    if [[ -n ${SECRET_TTY_STATE:-$TTY_STATE} ]];then stty "${SECRET_TTY_STATE:-$TTY_STATE}" <&3 2>/dev/null || :;fi
    if ((result));then log "Stopped during: $STAGE. Original credentials, state and staging are retained.";fi
    if ((result)) && [[ $FAILURE_CODE == 1 || $result != 2 ]];then result=1;fi
    exit "$result"
}
shell_quote() { printf "'%s'" "${1//\'/\'\\\'\'}"; }
ask_secret() {
    set +x
    local variable=$1 label=$2 character aborted=false
    local +x secret=''
    [[ $TTY == true && $NONINTERACTIVE == false ]] || fail "Interactive terminal required: $label"
    SECRET_TTY_STATE=$(stty -g <&3)
    stty -echo -icanon min 1 time 0 <&3
    printf '%s: ' "$label" >&3
    while :;do
        if ! IFS= read -r -n 1 character <&3;then aborted=true;break;fi
        case $character in
            ''|$'\r') break;;
            $'\x04') aborted=true;break;;
            $'\x08'|$'\x7f') secret=${secret%?};;
            $'\x15') secret='';;
            $'\x17')
                while [[ $secret == *[[:space:]] ]];do secret=${secret%?};done
                while [[ -n $secret && $secret != *[[:space:]] ]];do secret=${secret%?};done;;
            *) secret+="$character";;
        esac
    done
    stty "$SECRET_TTY_STATE" <&3;SECRET_TTY_STATE=''
    printf '\n' >&3
    [[ $aborted == false ]] || fail 'Password input closed'
    printf -v "$variable" '%s' "$secret"
}
remote_sudo() {
    set +x
    local target=$1 command=$2 wrapped
    if [[ ${target%@*} == root ]];then
        ssh "${SSH_ARGS[@]}" "$target" "$command" </dev/null
        return
    fi
    # Neither a password nor pending terminal input may reach the installed program,
    # including when sudo policy changes and stops consuming its password input.
    wrapped=$(shell_quote "exec </dev/null; $command")
    if [[ ${SUDO_MODES[$target]:-} == password ]];then
        printf '%s\n' "${SUDO_PASSWORDS[$target]}" | ssh "${SSH_ARGS[@]}" "$target" "sudo -S -k -p '' -- bash -c $wrapped"
    else
        ssh "${SSH_ARGS[@]}" "$target" "sudo -n -k -- bash -c $wrapped" </dev/null
    fi
}
remote_sudo_preflight() {
    set +x
    local target=$1 attempt
    local +x sudo_secret
    [[ ${target%@*} != root ]] || return 0
    if [[ ${SUDO_MODES[$target]:-} == password ]];then remote_sudo "$target" true;return;fi
    if ssh "${SSH_ARGS[@]}" "$target" "sudo -n -k -- bash -c 'exec </dev/null; true'" </dev/null 2>/dev/null;then SUDO_MODES[$target]=nopass;return;fi
    [[ $TTY == true && $NONINTERACTIVE == false ]] || fail "Target $target needs a sudo password; use an interactive terminal without --non-interactive, or configure passwordless sudo"
    for attempt in 1 2 3;do
        ask_secret sudo_secret "sudo password / sudo 密码 for $target"
        SUDO_PASSWORDS[$target]=$sudo_secret
        SUDO_MODES[$target]=password
        sudo_secret=''
        if remote_sudo "$target" true;then return;fi
        unset 'SUDO_PASSWORDS[$target]' 'SUDO_MODES[$target]'
        log "sudo authentication failed for $target (attempt $attempt/3); check the password and sudo permission"
    done
    fail "Cannot obtain sudo privileges on $target"
}
validate_target() { valid_target "$1" || fail "SSH target must be local or user@host: $1"; }

member_ip() {
    local endpoint=${1#*=}
    if [[ $endpoint == \[*\]:* ]]; then endpoint=${endpoint#\[};printf '%s' "${endpoint%\]:*}"
    else printf '%s' "${endpoint%:*}";fi
}
scp_target() {
    local user=${1%@*} host=${1#*@}
    host=${host#\[};host=${host%\]}
    if [[ $host == *:* ]]; then printf '%s@[%s]' "$user" "$host";else printf '%s' "$1";fi
}
remote_preflight() {
    local target=$1 machine
    validate_target "$target"
    SSH_TARGETS[$target]=1
    log "SSH connection / SSH 连接: $target ($SSH_AUTH)"
    machine=$(ssh "${SSH_ARGS[@]}" "$target" 'test "$(uname -s)" = Linux && command -v bash >/dev/null && command -v systemctl >/dev/null && command -v timeout >/dev/null && test -d /run/systemd/system && uname -m')
    [[ $machine == "$(uname -m)" ]] || fail "Remote OS/architecture differs from the selected binary: $target"
    remote_sudo_preflight "$target"
}
controller_tcp_probe() {
    local source=$1 endpoint=$2 host port code
    host=$(member_ip "$endpoint");port=${endpoint##*:}
    code="exec 3<>/dev/tcp/$host/$port"
    if [[ $source == local ]];then timeout 2 bash -c "$code" </dev/null >/dev/null 2>&1
    else ssh "${SSH_ARGS[@]}" "$source" "timeout 2 bash -c $(shell_quote "$code")" </dev/null >/dev/null 2>&1;fi
}
check_controller_reachability() {
    local value source endpoint failed=0
    local -a sources=(local)
    local -A seen=([local]=1)
    for value in "${CONTROLLERS[@]}" "${AGENTS[@]}";do
        source=${value#*=}
        if [[ ! ${seen[$source]+present} ]];then seen[$source]=1;sources+=("$source");fi
    done
    log 'Checking controller TCP reachability from the operator and target hosts / 检查操作机及各节点到控制器的 TCP 连接'
    for source in "${sources[@]}";do
        for endpoint in "${MEMBERS[@]}";do
            if controller_tcp_probe "$source" "$endpoint";then continue;fi
            sleep 1
            if controller_tcp_probe "$source" "$endpoint";then continue;fi
            log "TCP unreachable / TCP 不可达: $source -> ${endpoint#*=}"
            failed=1
        done
    done
    if ((failed));then
        log '检查控制器监听地址、路由和防火墙；修复后使用原安装记录及 --resume。安装器不会修改防火墙 / Check controller listeners, routes and firewall; resume the same record after repair.'
        return 1
    fi
}
remote_install() {
    local target=$1 role=$2 input=$3 checkpoint=$4 remote destination flag='' line
    if [[ -f $checkpoint ]]; then
        line=$(cat "$checkpoint");remote=${line#*$'\t'}
        [[ $line == "$target"$'\t'* && $remote =~ ^/tmp/converyn-install\.[A-Za-z0-9]+$ ]] || fail 'Invalid remote staging checkpoint'
        flag=--resume
        if ! ssh "${SSH_ARGS[@]}" "$target" "test -d '$remote'";then
            remote=$(ssh "${SSH_ARGS[@]}" "$target" 'umask 077; mktemp -d /tmp/converyn-install.XXXXXXXX')
            [[ $remote =~ ^/tmp/converyn-install\.[A-Za-z0-9]+$ ]] || fail 'Invalid replacement remote staging path'
            mark "$checkpoint" "$target"$'\t'"$remote"
        fi
    else
        remote=$(ssh "${SSH_ARGS[@]}" "$target" 'umask 077; mktemp -d /tmp/converyn-install.XXXXXXXX')
        [[ $remote =~ ^/tmp/converyn-install\.[A-Za-z0-9]+$ ]] || fail 'Invalid remote staging path'
        mark "$checkpoint" "$target"$'\t'"$remote"
    fi
    destination=$(scp_target "$target")
    scp "${SCP_ARGS[@]}" -- "$INSTALL_SCRIPT" "$BINARY" "$destination:$remote/"
    if [[ $role == controller ]]; then
        # Only this controller's material travels to the host, never the whole CA/operator bundle.
        tar -C "$input" -cf "$DIRECTORY/controller-transfer.tar" .
        scp "${SCP_ARGS[@]}" -- "$DIRECTORY/controller-transfer.tar" "$destination:$remote/bundle.tar"
        ssh "${SSH_ARGS[@]}" "$target" "umask 077; mkdir -p '$remote/controller'; tar -xf '$remote/bundle.tar' -C '$remote/controller'"
        line="--bundle '$remote/controller'"
    else
        scp "${SCP_ARGS[@]}" -- "$input" "$destination:$remote/bootstrap.json"
        line="--bootstrap '$remote/bootstrap.json'"
        [[ -z $HARDWARE_FILE ]] || line+=" --hardware-id-file '$HARDWARE_FILE'"
    fi
    local host_options="--selinux '$SELINUX_MODE'"
    [[ $role != controller ]] || host_options+=" --firewall '$FIREWALL_MODE'"
    remote_sudo "$target" "bash '$remote/install.sh' '$role' --binary '$remote/converyn' --sha256 '$(file_sha "$BINARY")' $host_options --yes --non-interactive $flag $line"
    # Retain the root-private staging/checkpoint for explicit resume; it is part of the installation record.
}
wait_for() {
    local description=$1;shift
    local attempt
    for ((attempt=1;attempt<=30;attempt++));do
        if "$@" > "$DIRECTORY/last-check.log" 2>&1;then return 0;fi
        if ((attempt==1 || attempt%5==0));then log "Waiting for $description (attempt $attempt/30) / 正在等待";fi
        sleep 2
    done
    log "$description failed; private diagnostic: $DIRECTORY/last-check.log"
    return 1
}
bootstrap_quorum_ready() {
    local entry
    local +x token
    token=$(cat "$BUNDLE/admin/token")
    for entry in "${MEMBERS[@]}";do
        if printf 'Authorization: Bearer %s\n' "$token" | curl -q --fail --silent --show-error --noproxy '*' --proto '=https' \
            --connect-timeout 3 --max-time 6 --cacert "$BUNDLE/admin/ca.crt" --cert "$BUNDLE/admin/tls.crt" --key "$BUNDLE/admin/tls.key" \
            --header @- --header "x-converyn-cluster: $CLUSTER" "https://${entry#*=}/health/ready";then return 0;fi
    done
    return 1
}
agent_observed() {
    local node=$1 config=${2:-$DIRECTORY/operator/config.json} context=${3:-} observed now
    local -a args=(--config "$config")
    [[ -z $context ]] || args+=(--context "$context")
    "$BINARY" "${args[@]}" -o json get nodes "$node" > "$DIRECTORY/agent-$node.observation.json" || return 1
    # The native -o json formatter puts the selected Node field at depth three.
    # Do not mistake a nested telemetry timestamp for a fresh authenticated heartbeat.
    observed=$(grep -oE '^      "observed_ms"[[:space:]]*:[[:space:]]*[0-9]+' "$DIRECTORY/agent-$node.observation.json" | grep -oE '[0-9]+$') || return 1
    [[ $observed =~ ^[0-9]{1,16}$ ]] || { log "Agent $node has no valid heartbeat timestamp";return 1; }
    now=$(date +%s%3N)
    if (( now-observed<=30000 && observed-now<=30000 ));then return 0;fi
    log "Agent $node heartbeat is stale or host clocks differ; inspect the retained observation"
    return 1
}
validate_deployment() {
    local value id endpoint endpoint_key host target count=${#MEMBERS[@]}
    [[ $count == 1 || $count == 3 || $count == 5 ]] || fail 'Use exactly 1, 3 or 5 controller members'
    identifier "$CLUSTER"
    [[ -n $OPERATOR && ${#OPERATOR} -le 128 && $OPERATOR != -* && $OPERATOR != *$'\n'* && $OPERATOR != *$'\r'* && $OPERATOR != *$'\t'* ]] || fail 'Provide a valid --operator-name for the first administrator'
    declare -A seen_ids=() seen_endpoints=() targets=() agent_ids=() controller_hosts=() agent_hosts=()
    for value in "${MEMBERS[@]}";do
        id=${value%%=*};endpoint=${value#*=}
        [[ $id =~ ^[1-9][0-9]*$ && $id -le 2147483647 && $value == *=* && $endpoint != *[[:space:]]* ]] || fail "Invalid member: $value"
        valid_endpoint "$endpoint" || fail "Invalid controller address for member $id: $endpoint; use IP:PORT or [IPv6]:PORT"
        endpoint_key=$(canonical_ip "$(member_ip "$value")"):${endpoint##*:}
        [[ ! ${seen_ids[$id]+exists} && ! ${seen_endpoints[$endpoint_key]+exists} ]] || fail "Duplicate member ID/address: member $id, $endpoint"
        seen_ids[$id]=1;seen_endpoints[$endpoint_key]=1
    done
    if [[ ${#CONTROLLERS[@]} == 0 ]];then
        for value in "${MEMBERS[@]}";do
            id=${value%%=*}
            if [[ $count == 1 ]];then CONTROLLERS+=("$id=local")
            else [[ -n $SSH_USER ]] || fail 'Provide --ssh-user or explicit --controller ID=user@host targets';CONTROLLERS+=("$id=$SSH_USER@$(member_ip "$value")");fi
        done
    fi
    [[ ${#CONTROLLERS[@]} == "$count" ]] || fail 'Every member needs one --controller target'
    local locals=0
    for value in "${CONTROLLERS[@]}";do
        id=${value%%=*};target=${value#*=};validate_target "$target"
        [[ ${seen_ids[$id]+exists} && ! ${targets[$id]+exists} ]] || fail 'Controller target ID is missing or duplicated'
        host=$(target_host "$target")
        [[ ! ${controller_hosts[$host]+exists} ]] || fail "Controller $id uses $target, already assigned to controller ${controller_hosts[$host]}"
        controller_hosts[$host]=$id
        targets[$id]=$target
        if [[ $target == local ]];then locals=$((locals+1));fi
    done
    [[ $locals -le 1 ]] || fail 'Each controller must use a distinct host; only one may be local'
    for value in "${AGENTS[@]}";do
        [[ $value == *=* ]] || fail 'Agent input uses NODE=TARGET'
        id=${value%%=*};target=${value#*=};identifier "$id";validate_target "$target"
        [[ ! ${agent_ids[$id]+exists} ]] || fail 'Duplicate Agent UID'
        host=$(target_host "$target")
        [[ ! ${agent_hosts[$host]+exists} ]] || fail "Agent $id uses $target, already assigned to Agent ${agent_hosts[$host]}; use another host"
        agent_hosts[$host]=$id
        agent_ids[$id]=1
    done
}
plan_text() {
    printf 'cluster\t%s\noperator\t%s\nbinary_sha256\t%s\ninstaller_sha256\t%s\n' "$CLUSTER" "$OPERATOR" "$(file_sha "$BINARY")" "$(file_sha "$INSTALL_SCRIPT")"
    printf 'hardware_id_file\t%s\n' "$HARDWARE_FILE"
    printf 'member\t%s\n' "${MEMBERS[@]}"
    printf 'controller\t%s\n' "${CONTROLLERS[@]}"
    if [[ ${#AGENTS[@]} -gt 0 ]];then printf 'agent\t%s\n' "${AGENTS[@]}";fi
}
cleanup_cli_stage() {
    if [[ $CLI_STAGE == /tmp/converyn-cli-context.* && -d $CLI_STAGE && ! -L $CLI_STAGE ]];then rm -rf -- "$CLI_STAGE";fi
    CLI_STAGE='';CLI_CONTEXT_USER='';CLI_CONTEXT_BINARY=''
}
cli_context_command() {
    if [[ -n $CLI_CONTEXT_USER ]];then runuser -u "$CLI_CONTEXT_USER" -- "$CLI_CONTEXT_BINARY" "$@"
    else "$CLI_CONTEXT_BINARY" "$@";fi
}
install_default_context() {
    local target_home=${HOME:-} target_uid=$EUID target_gid target_user account name ignored target selected
    target_gid=$(id -g) || return 1
    target_user=$(id -un) || return 1
    if [[ $EUID == 0 && ${SUDO_UID:-0} =~ ^[0-9]+$ && ${SUDO_UID:-0} != 0 && -n ${SUDO_USER:-} ]];then
        account=$(getent passwd "$SUDO_UID") || return 1
        IFS=: read -r name ignored target_uid target_gid ignored target_home ignored <<< "$account"
        [[ $name == "$SUDO_USER" && $target_uid == "$SUDO_UID" && $target_home == /* ]] || return 1
        target_user=$name;CLI_CONTEXT_USER=$name
        command -v runuser >/dev/null 2>&1 || { log 'runuser is required to import into the sudo caller account';return 1; }
    fi
    if [[ ${CONVERYN_CONFIG+defined} ]];then
        [[ -n $CONVERYN_CONFIG ]] || { log 'CONVERYN_CONFIG is empty; unset it or choose a config path';return 1; }
        target=$CONVERYN_CONFIG
    else
        [[ $target_home == /* ]] || { log 'Cannot determine the operator home for default CLI configuration';return 1; }
        target=$target_home/.converyn/config.json
    fi
    [[ $target == /* ]] || target=$PWD/$target
    CLI_STAGE=$(mktemp -d /tmp/converyn-cli-context.XXXXXXXX) || return 1
    chmod 700 "$CLI_STAGE" || return 1
    # Normalize only the personal OP credentials. No CA/issuer signing key is delivered.
    if ! "$BINARY" --config "$CLI_STAGE/normalized.json" config import "$DIRECTORY/operator/config.json" --name "$CLUSTER" > /dev/null 2> "$CLI_STAGE/error";then
        log 'Cannot validate the installer-created personal config for local CLI import';return 1
    fi
    CLI_CONTEXT_BINARY=$BINARY
    if [[ -n $CLI_CONTEXT_USER ]];then
        cp -- "$BINARY" "$CLI_STAGE/converyn" || return 1
        chmod 700 "$CLI_STAGE/converyn" || return 1
        chown "$target_uid:$target_gid" "$CLI_STAGE/converyn" "$CLI_STAGE/normalized.json" || return 1
        # Hand off the directory only after all privileged copies/chowns are complete.
        chown "$target_uid:$target_gid" "$CLI_STAGE" || return 1
        CLI_CONTEXT_BINARY=$CLI_STAGE/converyn
    fi
    if [[ -e $target || -L $target ]];then
        if cli_context_command --config "$target" --context "$CLUSTER" config export --output "$CLI_STAGE/existing.json" > /dev/null 2>/dev/null;then
            if [[ ! -f $CLI_STAGE/existing.json || -L $CLI_STAGE/existing.json ]] || ! cmp -s -- "$CLI_STAGE/normalized.json" "$CLI_STAGE/existing.json";then
                log "Local context $CLUSTER already contains different credentials; the existing config was retained: $target"
                return 1
            fi
        elif ! cli_context_command --config "$target" config import "$CLI_STAGE/normalized.json" --name "$CLUSTER" > /dev/null 2>/dev/null;then
            log "Could not import the personal context into $target; the existing config was retained"
            return 1
        fi
    elif ! cli_context_command --config "$target" config import "$CLI_STAGE/normalized.json" --name "$CLUSTER" > /dev/null 2>/dev/null;then
        log "Could not create the operator's private CLI config: $target"
        return 1
    fi
    if selected=$(cli_context_command --config "$target" -o json config current-context 2>/dev/null);then
        selected=${selected#\"};selected=${selected%\"}
    else
        cli_context_command --config "$target" config use-context "$CLUSTER" > /dev/null 2>/dev/null || return 1
        selected=$CLUSTER
    fi
    log "Personal CLI context / 个人 CLI context: $CLUSTER; user: $target_user; config: $target"
    if [[ $selected == "$CLUSTER" ]];then
        log 'Default commands / 默认命令: converyn status; converyn get nodes; converyn get workflows; converyn get runs'
    else
        log "Existing current context retained: $selected"
        printf 'converyn --context %q status\nconveryn config use-context %q\n' "$CLUSTER" "$CLUSTER"
    fi
}
run_deployment() {
    local value id target entry bootstrap default_context_ready=false
    validate_deployment
    DIRECTORY=${DIRECTORY:-$(default_install_directory)};safe_path "$DIRECTORY"
    log "Action: deploy; cluster: $CLUSTER; installation: $DIRECTORY"
    for value in "${CONTROLLERS[@]}";do log "Controller $value";done
    for value in "${AGENTS[@]}";do log "Agent $value";done
    confirm
    FAILURE_CODE=1
    STAGE='validate binary bundle contract';validate_binary_contract
    if [[ -e $DIRECTORY ]];then [[ $RESUME == true ]] || fail 'Installation directory exists; use --resume with the same inputs';private_directory "$DIRECTORY"
    else private_directory "$DIRECTORY";fi
    need flock
    exec 9>"$DIRECTORY/installation.lock";flock -n 9 || fail 'Another deployment is using this installation record'
    if [[ -e $DIRECTORY/install.sh ]];then
        [[ $RESUME == true && ! -L $DIRECTORY/install.sh && $(file_sha "$DIRECTORY/install.sh") == "$(file_sha "$INSTALL_SCRIPT")" ]] || fail "Installer changed; use the retained $DIRECTORY/install.sh with its original binary"
    else cp -- "$INSTALL_SCRIPT" "$DIRECTORY/install.sh";chmod 700 "$DIRECTORY/install.sh";fi
    INSTALL_SCRIPT=$DIRECTORY/install.sh
    acquire_binary "$DIRECTORY/converyn"
    cleanup_binary_probe
    if [[ -f $DIRECTORY/plan.tsv ]];then
        cmp -s <(plan_text) "$DIRECTORY/plan.tsv" || fail 'Resume inputs differ from the original plan'
    else plan_text > "$DIRECTORY/plan.tsv";fi
    log "Cluster: $CLUSTER; private installation record: $DIRECTORY"
    log "Controller firewall: $FIREWALL_MODE; SELinux: $SELINUX_MODE"
    for value in "${CONTROLLERS[@]}";do log "Controller $value";done
    for value in "${AGENTS[@]}";do log "Agent $value";done
    STAGE='generate private controller bundles'; log "$STAGE"
    BUNDLE=$DIRECTORY/bundle
    local new_bundle=false
    if [[ ! -f $DIRECTORY/bundle.ready ]];then
        [[ ! -e $BUNDLE ]] || fail 'Bundle creation was interrupted; retain it for review and use a new directory for a fresh, uninitialized plan'
        local args=(admin init --directory "$BUNDLE" --cluster "$CLUSTER")
        for value in "${MEMBERS[@]}";do args+=(--member "$value");done
        "$BINARY" "${args[@]}" >/dev/null
        new_bundle=true
    else
        [[ -f $BUNDLE/members.json && ! -L $BUNDLE/members.json && $(cat "$DIRECTORY/bundle.ready") == "$(file_sha "$BUNDLE/members.json")" ]] || fail 'Prepared membership no longer matches its original checkpoint; retain the installation record for review'
    fi
    validate_prepared_bundle "$BUNDLE"
    if [[ $new_bundle == true ]];then mark "$DIRECTORY/bundle.ready" "$(file_sha "$BUNDLE/members.json")";fi
    if [[ $PREPARE_ONLY == true ]];then log "Prepared only; no host services or SSH connections changed. Bundle: $BUNDLE";return 0;fi
    local remote_needed=false
    for value in "${CONTROLLERS[@]}" "${AGENTS[@]}";do [[ ${value#*=} == local ]] || remote_needed=true;done
    if [[ $remote_needed == true ]];then
        need ssh;need scp;need tar
        prepare_known_hosts
        ssh_arguments
    fi
    need curl
    STAGE='preflight all target hosts'; log "$STAGE"
    for value in "${CONTROLLERS[@]}" "${AGENTS[@]}";do
        target=${value#*=};if [[ $target == local ]];then root_host;else remote_preflight "$target";fi
    done
    STAGE='install controller services'; log "$STAGE"
    for value in "${CONTROLLERS[@]}";do
        id=${value%%=*};target=${value#*=}
        if [[ $target == local ]];then BUNDLE=$DIRECTORY/bundle/controller-$id;install_controller
        else remote_install "$target" controller "$DIRECTORY/bundle/controller-$id" "$DIRECTORY/controller-$id.remote";fi
    done
    BUNDLE=$DIRECTORY/bundle
    STAGE='verify controller network reachability';log "$STAGE"
    check_controller_reachability
    STAGE='initialize the original member table'; log "$STAGE"
    if [[ ! -f $DIRECTORY/operator/operation.json ]];then
        wait_for 'Initial quorum' "$BINARY" --config "$BUNDLE/admin/config.json" admin initialize --members "$BUNDLE/members.json"
    fi
    STAGE='confirm a controller leader and quorum';log "$STAGE"
    if ! wait_for 'controller leader and quorum' bootstrap_quorum_ready;then
        log '控制面尚未确认 Leader 和多数派；检查控制器网络、TLS 与日志，然后使用原安装记录 --resume。已生成的身份和状态会保留。'
        return 1
    fi
    STAGE='register the named operator'; log "$STAGE"
    if [[ -f $DIRECTORY/operator/operation.json ]];then
        if ! wait_for 'resume original operator registration' "$BINARY" --config "$BUNDLE/admin/config.json" resume credentials --output "$DIRECTORY/operator";then
            log 'OP 原始注册请求尚未确认；保留 operator/operation.json，修复网络后用 --resume 重试。'
            return 1
        fi
    else
        [[ ! -e $DIRECTORY/operator ]] || fail 'Incomplete operator credential bundle retained; inspect it before recovery'
        if ! "$BINARY" create identities --config "$BUNDLE/admin/config.json" --authority "$BUNDLE/user-authority" --name "$OPERATOR" --super-admin --output "$DIRECTORY/operator" >/dev/null;then
            if [[ -f $DIRECTORY/operator/operation.json ]];then log 'OP 注册结果未确认；原证书和 operation.json 已保留，检查 Leader/网络后用 --resume 重试。'
            else log 'OP 目录不完整，已保留现场；请先检查该目录后再恢复。';fi
            return 1
        fi
    fi
    wait_for 'Authenticated quorum read' "$BINARY" --config "$DIRECTORY/operator/config.json" get members
    STAGE='verify every controller HTTPS entry'; log "$STAGE"
    printf 'Authorization: Bearer %s\n' "$(cat "$DIRECTORY/operator/token")" > "$DIRECTORY/operator-header"
    for entry in "${MEMBERS[@]}";do
        id=${entry%%=*}
        wait_for "Controller $id quorum-confirmed business read" curl -q --fail --silent --show-error --noproxy '*' --proto '=https' \
          --connect-timeout 5 --max-time 15 --cacert "$DIRECTORY/operator/ca.crt" --cert "$DIRECTORY/operator/tls.crt" --key "$DIRECTORY/operator/tls.key" \
          --header "@$DIRECTORY/operator-header" --header "x-converyn-cluster: $CLUSTER" "https://${entry#*=}/api/v1/workflows"
    done
    rm -f -- "$DIRECTORY/operator-header"
    STAGE='install and confirm Agents' ; log "$STAGE"
    for value in "${AGENTS[@]}";do
        id=${value%%=*};target=${value#*=};bootstrap=$DIRECTORY/agent-$id-bootstrap.json
        if [[ ! -f $bootstrap ]];then "$BINARY" --config "$DIRECTORY/operator/config.json" admin bootstrap --node "$id" --ttl-seconds 3600 --output "$bootstrap" >/dev/null;fi
        if [[ ! -f $DIRECTORY/agent-$id.complete ]];then
            if [[ $target == local ]];then BOOTSTRAP=$bootstrap;install_agent
            else remote_install "$target" agent "$bootstrap" "$DIRECTORY/agent-$id.remote";fi
        fi
        wait_for "Agent $id fresh authenticated heartbeat" agent_observed "$id"
        mark "$DIRECTORY/agent-$id.complete" "$(file_sha "$bootstrap")"
    done
    STAGE='verify installed cluster'; log "$STAGE"
    "$BINARY" --config "$DIRECTORY/operator/config.json" status
    mark "$DIRECTORY/installation.complete" "$(date -u +%FT%TZ)"
    if [[ $AUTO_DEFAULT_CONTEXT == true ]];then
        STAGE='configure the operator CLI context'
        if install_default_context;then
            default_context_ready=true
        else
            log 'Platform installation is complete; local CLI context import needs attention / 平台安装完成，默认 CLI 配置尚未导入。'
            log 'Import the existing operator/config.json with a new local context name; do not create another first identity.'
        fi
        cleanup_cli_stage
    fi
    log 'Installation complete.'
    if [[ $default_context_ready != true ]];then
        if [[ $EUID == 0 && -n ${SUDO_USER:-} && $SUDO_USER != root ]];then printf 'sudo ';fi
        printf 'converyn --config %q status\n' "$DIRECTORY/operator/config.json"
    fi
    log "Original personal config (keep private): $DIRECTORY/operator/config.json"
    log "Keep $DIRECTORY offline/private for identity, keyring and recovery. Agents trust approved workflows; Shell host steps explicitly declare execution: host."
}
interactive() {
    local choice index count address port target node any_remote=false directory_default ssh_default key
    local -A wizard_ips=() wizard_controller_hosts=() wizard_agent_ids=() wizard_agent_hosts=()
    ask_option choice '部署方式 / Mode: 1=本机一体 2=单控制器+多Agent 3=HA 4=仅Agent 5=仅CLI' 1 mode valid_mode
    case $choice in
        4) MODE=agent;ask_option BOOTSTRAP 'Private Agent bootstrap file / 引导文件' "$BOOTSTRAP" bootstrap valid_bootstrap;return;;
        5) MODE=client;return;;
        1|2|3) MODE=deploy;;
    esac
    ask_option CLUSTER 'Cluster name / 集群名称' "$CLUSTER" cluster valid_identifier
    ask_option OPERATOR 'Operator display name / 管理员名称' "$OPERATOR" operator valid_operator
    directory_default=$(default_install_directory)
    ask_option DIRECTORY 'Private installation directory / 私有安装目录' "${DIRECTORY:-$directory_default}" directory valid_install_directory
    if [[ $choice == 3 ]];then ask_option count 'Controller count / 控制器数量 (3 or 5)' 3 controller_count valid_controller_count;else count=1;fi
    if [[ $choice != 1 ]];then
        ssh_default=$SSH_USER
        ask_option SSH_USER 'SSH user / SSH 用户' "$ssh_default" ssh_user valid_ssh_user
        log '先填写控制器固定 IP；SSH 安装目标默认使用 用户@该IP，直接回车即可 / Enter the controller IP first, then accept or edit its SSH target'
    fi
    for ((index=1;index<=count;index++));do
        if [[ $choice == 1 ]];then address=127.0.0.1
        else ask_option address "Controller $index stable IP / 固定IP（必填）" '' "controller_${index}_ip" wizard_controller_ip;fi
        key=$(canonical_ip "$address");wizard_ips[$key]=$index
        ask_option port "Controller $index HTTPS port" 8443 "controller_${index}_port" valid_port
        if [[ $address == *:* ]];then MEMBERS+=("$index=[$address]:$port");else MEMBERS+=("$index=$address:$port");fi
        if [[ $choice == 3 ]];then
            target=$(normalize_target "$SSH_USER@$address")
            ask_option target "Controller $index target / 安装目标 (local、user@host 或 IP)" "$target" "controller_${index}_target" wizard_controller_target
        else target=local;fi
        wizard_controller_hosts[$(target_host "$target")]=$index
        CONTROLLERS+=("$index=$target")
    done
    if [[ $choice == 1 ]];then ask_option node 'Local Agent UID / 本机Agent名称' '' local_agent_uid valid_identifier;AGENTS+=("$node=local")
    else
        ask_option count 'Agent count / Agent数量 (0 allowed)' 0 agent_count valid_agent_count
        for ((index=1;index<=count;index++));do
            ask_option node "Agent $index UID" '' "agent_${index}_uid" wizard_agent_uid
            ask_option target "Agent $node target / 安装目标 (local、user@host 或 IP)" '' "agent_${index}_target" wizard_agent_target
            wizard_agent_ids[$node]=1;wizard_agent_hosts[$(target_host "$target")]=$node
            AGENTS+=("$node=$target")
        done
    fi
    for target in "${CONTROLLERS[@]}" "${AGENTS[@]}";do [[ ${target#*=} == local ]] || any_remote=true;done
    if [[ $any_remote == true ]];then
        ask_option KNOWN_HOSTS 'SSH known_hosts file / 主机指纹文件（首次可创建）' "${KNOWN_HOSTS:-$HOME/.ssh/known_hosts}" known_hosts valid_known_hosts
        ask_option SSH_AUTH 'SSH authentication / SSH认证: auto=密钥或密码 key=仅密钥 password=密码' "$SSH_AUTH" ssh_auth wizard_ssh_auth
        if [[ $SSH_AUTH != password ]];then ask_option SSH_KEY 'SSH key file (empty uses ssh-agent or password in auto mode)' "$SSH_KEY" ssh_key valid_optional_key;fi
    fi
}
parse() {
    if [[ ${1:-} =~ ^(deploy|controller|agent|client)$ ]];then MODE=$1;shift;fi
    while (($#));do
        [[ $1 != --* ]] || SUPPLIED[$1]=1
        case $1 in
            --binary|--binary-url|--sha256|--installation|--cluster|--operator-name|--bundle|--bootstrap|--hardware-id-file|--ssh-key|--ssh-known-hosts|--ssh-user|--ssh-port|--ssh-auth|--input-cache|--firewall|--selinux|--member|--controller|--agent)
                [[ $# -ge 2 && -n $2 && $2 != --* ]] || fail "Missing value for $1"
                case $1 in
                    --binary) CLI_SET[BINARY]=1;; --binary-url) CLI_SET[BINARY_URL]=1;; --sha256) CLI_SET[EXPECTED_SHA]=1;; --installation) CLI_SET[DIRECTORY]=1;; --cluster) CLI_SET[CLUSTER]=1;; --operator-name) CLI_SET[OPERATOR]=1;; --ssh-user) CLI_SET[SSH_USER]=1;; --ssh-auth) CLI_SET[SSH_AUTH]=1;; --ssh-key) CLI_SET[SSH_KEY]=1;; --ssh-known-hosts) CLI_SET[KNOWN_HOSTS]=1;; --bootstrap) CLI_SET[BOOTSTRAP]=1;;
                esac
                case $1 in
                    --input-cache) INPUT_CACHE=$2;; --binary) BINARY=$2;; --binary-url) BINARY_URL=$2;; --sha256) EXPECTED_SHA=$2;; --installation) DIRECTORY=$2;;
                    --firewall) FIREWALL_MODE=$2;; --selinux) SELINUX_MODE=$2;;
                    --cluster) CLUSTER=$2;; --operator-name) OPERATOR=$2;; --bundle) BUNDLE=$2;; --bootstrap) BOOTSTRAP=$2;; --hardware-id-file) HARDWARE_FILE=$2;; --ssh-key) SSH_KEY=$2;; --ssh-known-hosts) KNOWN_HOSTS=$2;; --ssh-user) SSH_USER=$2;; --ssh-port) SSH_PORT=$2;; --ssh-auth) SSH_AUTH=$2;;
                    --member) MEMBERS+=("$2");; --controller) CONTROLLERS+=("$2");; --agent) AGENTS+=("$2");;
                esac;shift;;
            --no-default-context) AUTO_DEFAULT_CONTEXT=false;; --no-input-cache) INPUT_CACHE_ENABLED=false;; --yes) ASSUME_YES=true;; --non-interactive) NONINTERACTIVE=true;; --prepare-only) PREPARE_ONLY=true;; --resume) RESUME=true;;
            --help|-h) usage;exit 0;; *) fail "Unknown option: $1";;
        esac;shift
    done
}
validate_options() {
    local option allowed
    for option in "${!SUPPLIED[@]}";do
        allowed=false
        case $option in
            --binary|--binary-url|--sha256|--yes|--non-interactive|--help|--no-input-cache) allowed=true;;
            --installation) [[ $MODE == deploy || -n $BINARY_URL ]] && allowed=true;;
            --cluster|--operator-name|--member|--controller|--agent|--prepare-only|--no-default-context)
                [[ $MODE == deploy ]] && allowed=true;;
            --bundle) [[ $MODE == controller ]] && allowed=true;;
            --firewall) [[ $MODE == deploy || $MODE == controller ]] && allowed=true;;
            --selinux) [[ $MODE != client ]] && allowed=true;;
            --resume) [[ $MODE != client || -n $BINARY_URL ]] && allowed=true;;
            --bootstrap) [[ $MODE == agent ]] && allowed=true;;
            --hardware-id-file) [[ $MODE == deploy || $MODE == agent ]] && allowed=true;;
            --ssh-auth|--ssh-user|--ssh-key|--ssh-known-hosts|--ssh-port) [[ $MODE == deploy ]] && allowed=true;;
            --input-cache) [[ $TTY == true && $NONINTERACTIVE == false ]] && allowed=true;;
        esac
        [[ $allowed == true ]] || fail "Option $option is not applicable to $MODE"
    done
}
summary() {
    printf 'Action: install %s\nResult: %s\n' "$MODE" "$(if [[ $PREPARE_ONLY == true ]];then printf prepared;else printf succeeded;fi)"
    [[ -z $DIRECTORY ]] || printf 'Installation: %s\n' "$DIRECTORY"
    case $MODE in
        deploy)
            if [[ $PREPARE_ONLY == true ]];then printf 'Output: %s/bundle\n' "$DIRECTORY"
            else printf 'Config: %s/operator/config.json\nNext: converyn --config %q status\n' "$DIRECTORY" "$DIRECTORY/operator/config.json";fi;;
        controller|agent) printf 'Next: systemctl status converyn-%s\n' "$MODE";;
        client) printf 'Output: /usr/local/bin/converyn\nNext: converyn --help\n';;
    esac
}
main() {
    [[ ${BASH_VERSINFO[0]} -gt 4 || ( ${BASH_VERSINFO[0]} -eq 4 && ${BASH_VERSINFO[1]} -ge 4 ) ]] || fail 'Bash 4.4 or newer is required'
    if [[ -t 0 && -t 2 ]];then exec 3<>/dev/tty;TTY=true;fi
    FAILURE_CODE=2
    parse "$@"
    validate_options
    trap installer_cleanup EXIT
    if [[ $TTY == false ]];then
        [[ $ASSUME_YES == true ]] || fail 'A terminal or --yes is required; use --yes --non-interactive for unattended execution'
    fi
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM
    for cmd in readlink sha256sum cut od tr install stat cmp cp mv find sort xargs grep date mkdir mktemp chmod rm timeout;do need "$cmd";done
    if [[ $TTY == true && $NONINTERACTIVE == false ]];then TTY_STATE=$(stty -g <&3 2>/dev/null || :);cache_init;fi
    if [[ $MODE == deploy && ${#MEMBERS[@]} == 0 ]];then interactive;fi
    validate_options
    if [[ $MODE == deploy ]];then
        [[ -n $CLUSTER ]] || ask_option CLUSTER 'Cluster name / 集群名称' '' cluster valid_identifier
        [[ -n $OPERATOR ]] || ask_option OPERATOR 'Operator display name / 管理员名称 (--operator-name)' '' operator valid_operator
    elif [[ $MODE == controller ]];then
        [[ -n $BUNDLE ]] || ask BUNDLE 'Controller bundle directory'
        [[ -n $BUNDLE && -d $BUNDLE ]] || fail '--bundle requires an existing controller directory'
        validate_controller_profile "$BUNDLE"
    elif [[ $MODE == agent ]];then
        [[ -n $BOOTSTRAP ]] || ask BOOTSTRAP 'Private Agent bootstrap file'
        [[ -n $BOOTSTRAP ]] || fail '--bootstrap is required for the Agent role'
        private_file "$BOOTSTRAP"
    fi
    [[ $SSH_PORT =~ ^[0-9]{1,5}$ ]] && ((10#$SSH_PORT>=1 && 10#$SSH_PORT<=65535)) || fail 'Invalid SSH port'
    [[ $SSH_AUTH == auto || $SSH_AUTH == key || $SSH_AUTH == password ]] || fail '--ssh-auth must be auto, key or password'
    [[ $FIREWALL_MODE == auto || $FIREWALL_MODE == skip ]] || fail '--firewall must be auto or skip'
    [[ $SELINUX_MODE == keep || $SELINUX_MODE == permissive ]] || fail '--selinux must be keep or permissive'
    [[ $SSH_AUTH != password || -z $SSH_KEY ]] || fail '--ssh-key cannot be combined with --ssh-auth password'
    [[ -z $SSH_USER || $SSH_USER =~ ^[A-Za-z_][A-Za-z0-9_-]*$ ]] || fail 'Invalid SSH user'
    [[ -z $SSH_KEY || -f $SSH_KEY ]] || fail 'SSH key selector is not a file'
    [[ -z $HARDWARE_FILE ]] || safe_path "$HARDWARE_FILE"
    [[ -z $EXPECTED_SHA || $EXPECTED_SHA =~ ^[a-fA-F0-9]{64}$ ]] || fail 'SHA-256 must contain 64 hexadecimal characters'
    [[ -z $SSH_KEY ]] || SSH_KEY=$(readlink -f -- "$SSH_KEY")
    [[ -z $KNOWN_HOSTS || $KNOWN_HOSTS == /* ]] || KNOWN_HOSTS=$PWD/$KNOWN_HOSTS
    select_binary
    [[ -z $BINARY ]] || cache_set binary "$BINARY"
    case $MODE in
        deploy) run_deployment >&2;;
        controller|agent|client)
            log "Action: $MODE; installation: ${DIRECTORY:-none}; bundle: ${BUNDLE:-none}; bootstrap: ${BOOTSTRAP:-none}"
            confirm
            FAILURE_CODE=1
            if [[ -n $BINARY_URL ]];then
                DIRECTORY=${DIRECTORY:-/secure/converyn-binary-install};private_directory "$DIRECTORY";acquire_binary "$DIRECTORY/converyn"
            fi
            if [[ $MODE != client ]];then
                root_host
                STAGE='validate binary bundle contract';validate_binary_contract;cleanup_binary_probe
            fi
            case $MODE in controller) install_controller;;agent) install_agent;;client) root_binary_host;install_binary;log 'CLI installed at /usr/local/bin/converyn';;esac >&2;;
    esac
    summary
}
if [[ ${BASH_SOURCE[0]} == "$0" ]];then main "$@";fi
