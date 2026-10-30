#!/usr/bin/env bash
# Personal and Node credentials are created by the existing Converyn CLI.
# Role grants are separate from certificate issuance and remain server-authorized.
set +x
set -Eeuo pipefail
umask 077
INIT_SCRIPT=$(readlink -f -- "${BASH_SOURCE[0]}")
ACTION=wizard BINARY='' INSTALLATION='' ADMIN_CONFIG='' ADMIN_CONTEXT='' AUTHORITY=''
CONTEXTS=${CONVERYN_CONFIG:-}
CONTEXT_NAME='' FILE='' SOURCE_CONTEXT='' OUTPUT='' NAME='' ROLE=executor WORKFLOW=''
VALID_FOR=365d NODE='' HARDWARE_FILE='' PASSWORD_FILE='' TTL=3600
NODE_SSH_HOST='' NODE_SSH_USER='' NODE_SSH_KEY='' NODE_SSH_KNOWN_HOSTS='' NODE_SSH_PORT=22 NODE_SSH_AUTH=auto
YES=false NONINTERACTIVE=false RESUME=false USE=false REPLACE=false ALL_WORKFLOWS=false
PRESETS=() TEMPORARIES=() TTY=false
STAGE='validate initialization inputs'
FAILURE_CODE=1
declare -A SUPPLIED=()
log(){ printf '[converyn-init] %s\n' "$*" >&2; }
fail(){ log "$*";exit "$FAILURE_CODE"; }
need(){ command -v "$1" >/dev/null 2>&1 || fail "Required command missing: $1"; }
identifier(){ [[ $1 =~ ^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$ ]] || fail "Invalid ID/context name: $1"; }
sha(){ sha256sum -- "$1" | cut -d ' ' -f 1; }
private_directory(){
    local path=$1 current owner mode
    [[ $path =~ ^/[A-Za-z0-9_./-]+$ && $path != / && $path != */../* && $path != */./* && $path != */.. && $path != */. && $path != *//* ]] || fail 'Use a normalized absolute private directory without spaces'
    current=$path
    while [[ $current != / ]];do
        [[ ! -L $current ]] || fail "Private directory traverses a symlink: $current"
        if [[ -e $current ]];then
            owner=$(stat -c %u -- "$current");mode=$(stat -c %a -- "$current")
            [[ $owner == "$EUID" || $owner == 0 ]] || fail "Another user owns private directory ancestor: $current"
            (( (8#$mode & 0022)==0 || (owner==0 && (8#$mode & 01000)!=0) )) || fail "Unprotected directory ancestor: $current"
        fi
        current=${current%/*};[[ -n $current ]] || current=/
    done
    mkdir -p -- "$path"
    [[ $(stat -c '%u:%a' -- "$path") == "$EUID:700" ]] || fail "Expected current-user directory mode 0700: $path"
}
private_file(){
    local owner
    [[ -f $1 && ! -L $1 && $(stat -c %a -- "$1") == 600 ]] || fail "Expected private regular file mode 0600: $1"
    owner=$(stat -c %u -- "$1")
    [[ $owner == "$EUID" || ( $EUID == 0 && $owner == "${SUDO_UID:-0}" ) ]] || fail "Private input belongs to another user: $1"
}
ask(){
    local variable=$1 message=$2 default=${3:-} value
    [[ $TTY == true && $NONINTERACTIVE == false ]] || fail "Missing input: $message"
    bind 'set editing-mode emacs' 2>/dev/null || :
    bind '"\C-h": backward-delete-char' 2>/dev/null || :
    bind '"\C-?": backward-delete-char' 2>/dev/null || :
    IFS= read -r -e -i "$default" -p "$message: " value <&3 2>&3 || { log 'Interactive input closed';exit 1; }
    value=${value#"${value%%[![:space:]]*}"};value=${value%"${value##*[![:space:]]}"}
    printf -v "$variable" '%s' "$value"
}
confirm(){ local answer;[[ $YES == false ]] || return 0;ask answer 'Proceed / 执行? (yes/no)' no;[[ $answer == yes ]] || { log 'Initialization canceled';exit 1; }; }
cleanup(){
    local result=$? directory
    for directory in "${TEMPORARIES[@]}";do
        if [[ $directory =~ ^/tmp/converyn-init\.[A-Za-z0-9]+$ ]];then rm -rf -- "$directory";fi
    done
    if ((result));then
        log "Stopped during: $STAGE. Existing credentials and contexts are retained."
        [[ $FAILURE_CODE == 2 && $result == 2 ]] || result=1
    fi
    trap - EXIT
    exit "$result"
}
usage(){
    cat <<'USAGE'
Usage: bash init.sh [import|operator|user|node|browser] [options]
No action opens the interactive initialization wizard.

Connection and files:
  --binary FILE                Converyn binary (otherwise discover installed/release binary)
  --installation DIR           install.sh record; reuses operator/config.json and bundle/user-authority
  --admin-config FILE          Explicit personal admin config, or first-OP bootstrap config
  --admin-context NAME         Select the admin's context in --admin-config
  --authority DIR              Offline personal user-authority directory; never distribute it
  --contexts FILE              Destination managed contexts (default: CONVERYN_CONFIG or ~/.converyn/config.json)
  --context-name NAME          Local alias for an imported/issued personal identity
  --file FILE                  Existing personal config for import/browser
  --source-context NAME        Select an input context when --file contains several
  --output DIR                 New private credential, node-delivery or browser directory

Personal credentials:
  --name DISPLAY_NAME          Display name for a new identity
  --role ROLE                  viewer, executor, maintainer, platform_operator, access_admin, super_admin
  --workflow ID                Scope a viewer/executor/maintainer grant to one workflow
  --preset NAME                Restrict viewer/executor to presets; repeat as needed
  --all-workflows              Explicitly grant a business role over all workflows
  --valid-for DURATION         Certificate lifetime (default: 365d)
  --use                        Select the imported context
  --replace-context            Explicitly replace this alias; never changes other contexts

Node delivery:
  --node UID                   New Node UID (no automatic identity recovery)
  --hardware-id-file FILE      Existing unique identity selector on the Agent host
  --ttl-seconds SECONDS        One-use bootstrap lifetime, 60..3600 (default: 3600)
  --ssh-host HOST              Install this new Node on a reachable SSH host
  --ssh-user USER              SSH user (required with --ssh-host)
  --ssh-auth auto|key|password Authentication (password requires a terminal)
  --ssh-key FILE               Optional private key selector
  --ssh-known-hosts FILE       Verified host keys (default: ~/.ssh/known_hosts)
  --ssh-port PORT              SSH port (default: 22)
Without --ssh-host, only prepare the private Node delivery. With --installation,
Node output defaults to INSTALLATION/nodes/UID. Remote success requires a fresh
authenticated heartbeat; failed requests retain their original delivery for --resume.

Browser:
  --password-file FILE          Private P12 password file; omit for native OpenSSL prompt

  --resume                     Reuse a recorded request/output rather than mint new credentials
  --yes --non-interactive      Run with complete explicit inputs, without prompts
  --help, -h                   Show this help

Binary discovery: explicit --binary, PATH, adjacent release binary, repository build.
Progress/errors go to stderr; stdout contains the final text result. Exit: 0 success/help, 2 input error, 1 execution failure/cancel.
--non-interactive forbids prompts; --yes only accepts the plan. Use both for unattended execution.
Options must apply to the selected action; credential/Node issuance supports --resume.

This is pure Bash plus system tools and the Converyn CLI; no Python or jq.
Only browser P12 export additionally requires installed OpenSSL.
USAGE
}
admin(){
    local args=(--config "$ADMIN_CONFIG")
    [[ -z $ADMIN_CONTEXT ]] || args+=(--context "$ADMIN_CONTEXT")
    "$BINARY" "${args[@]}" "$@"
}
input(){
    local args=(--config "$FILE")
    [[ -z $SOURCE_CONTEXT ]] || args+=(--context "$SOURCE_CONTEXT")
    "$BINARY" "${args[@]}" "$@"
}
select_inputs(){
    local candidate architecture directory=${INIT_SCRIPT%/*}
    if [[ -z $BINARY ]];then
        BINARY=$(command -v converyn || true)
        case $(uname -m) in x86_64) architecture=amd64;;aarch64|arm64) architecture=arm64;;*) architecture=unknown;;esac
        if [[ -z $BINARY ]];then
            for candidate in "$directory/converyn-linux-$architecture" "$directory/converyn" "$directory/../../target/release/converyn" "$directory/../../target/debug/converyn";do
                if [[ -x $candidate && -f $candidate ]];then BINARY=$candidate;break;fi
            done
        fi
        [[ -n $BINARY ]] || ask BINARY 'Converyn binary path'
    fi
    BINARY=$(readlink -f -- "$BINARY") || fail 'Binary path is missing or invalid';[[ -x $BINARY && -f $BINARY ]] || fail 'Converyn binary is not executable'
    "$BINARY" --help >/dev/null
    if [[ -n $INSTALLATION ]];then
        [[ -d $INSTALLATION ]] || fail 'Installation record directory is missing'
        INSTALLATION=$(readlink -f -- "$INSTALLATION")
        AUTHORITY=${AUTHORITY:-$INSTALLATION/bundle/user-authority}
        if [[ -z $ADMIN_CONFIG ]];then
            if [[ -f $INSTALLATION/operator/config.json ]];then ADMIN_CONFIG=$INSTALLATION/operator/config.json
            else ADMIN_CONFIG=$INSTALLATION/bundle/admin/config.json;fi
        fi
        if [[ $ACTION == import && -z $FILE && -f $INSTALLATION/operator/config.json ]];then FILE=$INSTALLATION/operator/config.json;fi
    fi
    if [[ -z $CONTEXTS && -n ${HOME:-} ]];then CONTEXTS=$HOME/.converyn/config.json;fi
    if [[ -n $CONTEXTS ]];then CONTEXTS=$(readlink -m -- "$CONTEXTS")
    elif [[ $ACTION != node && $ACTION != browser ]];then fail 'HOME is unset; provide --contexts FILE';fi
}
require_admin(){
    [[ -n $ADMIN_CONFIG ]] || ask ADMIN_CONFIG 'Existing admin personal config'
    private_file "$ADMIN_CONFIG"
}
require_authority(){
    [[ -n $AUTHORITY ]] || ask AUTHORITY 'Offline personal user-authority directory'
    [[ -f $AUTHORITY/ca.crt && -f $AUTHORITY/ca.key ]] || fail 'The personal issuing authority is incomplete'
    private_file "$AUTHORITY/ca.key"
}
record_plan(){
    local plan=$1 text=$2
    private_directory "${plan%/*}"
    exec 9>"$plan.lock";flock -n 9 || fail 'Another initialization uses this output'
    if [[ -e $plan ]];then
        [[ $RESUME == true && ! -L $plan ]] || fail 'Initialization record exists; use --resume with original inputs'
        cmp -s <(printf '%s\n' "$text") "$plan" || fail 'Resume inputs differ from the original request'
    else printf '%s\n' "$text" > "$plan";fi
}
import_context(){
    local temp exported=false
    [[ -n $FILE ]] || ask FILE 'Personal config to import'
    [[ -n $CONTEXT_NAME ]] || ask CONTEXT_NAME 'Local context alias' qa-op
    identifier "$CONTEXT_NAME";private_file "$FILE"
    # Connectivity alone also succeeds for bootstrap credentials. A context
    # intended for a person must resolve to the server's named identity object.
    if ! input -o json auth doctor | grep -E '^  "identity":[[:space:]]*[{]' >/dev/null;then
        fail 'Import requires an active named personal credential, not bootstrap or Agent material'
    fi
    private_directory "${CONTEXTS%/*}"
    temp=$(mktemp -d /tmp/converyn-init.XXXXXXXX);TEMPORARIES+=("$temp")
    local args=(--config "$temp/normalized.json")
    [[ -z $SOURCE_CONTEXT ]] || args+=(--context "$SOURCE_CONTEXT")
    "$BINARY" "${args[@]}" config import "$FILE" --name "$CONTEXT_NAME" >/dev/null
    if [[ -f $CONTEXTS ]];then
        if "$BINARY" --config "$CONTEXTS" --context "$CONTEXT_NAME" config export --output "$temp/current.json" >/dev/null 2>&1;then exported=true;fi
    fi
    if [[ $exported == true ]] && cmp -s "$temp/normalized.json" "$temp/current.json";then
        log "Context $CONTEXT_NAME already contains these exact credentials"
    else
        args=(--config "$CONTEXTS")
        [[ -z $SOURCE_CONTEXT ]] || args+=(--context "$SOURCE_CONTEXT")
        args+=(config import "$FILE" --name "$CONTEXT_NAME")
        [[ $REPLACE == false ]] || args+=(--replace)
        "$BINARY" "${args[@]}" >/dev/null
    fi
    [[ $USE == false ]] || "$BINARY" --config "$CONTEXTS" config use-context "$CONTEXT_NAME" >/dev/null
    printf 'converyn --config %q --context %q auth status\n' "$CONTEXTS" "$CONTEXT_NAME"
}
validate_scope(){
    case $ROLE in
        viewer|executor|maintainer)
            [[ -z $WORKFLOW || $ALL_WORKFLOWS == false ]] || fail 'Choose one --workflow or --all-workflows'
            [[ -n $WORKFLOW || $ALL_WORKFLOWS == true ]] || fail 'Business roles need --workflow or explicit --all-workflows'
            [[ -z $WORKFLOW ]] || identifier "$WORKFLOW"
            [[ $ROLE != maintainer || ${#PRESETS[@]} == 0 ]] || fail 'Maintainers manage a workflow across all its presets'
            ;;
        platform_operator|access_admin|super_admin)
            [[ -z $WORKFLOW && $ALL_WORKFLOWS == false && ${#PRESETS[@]} == 0 ]] || fail 'Platform/access/super administrator grants are instance-scoped'
            ;;
        *) fail 'Unknown role';;
    esac
    local preset
    for preset in "${PRESETS[@]}";do identifier "$preset";done
}
issue_user(){
    local first=$1 plan identity binding text
    require_admin;require_authority
    [[ -n $NAME && -n $CONTEXT_NAME && -n $OUTPUT ]] || fail 'Issuance requires --name, --context-name and --output'
    identifier "$CONTEXT_NAME"
    [[ $NAME != *[[:cntrl:]]* && $NAME != -* && -n ${NAME//[[:space:]]/} ]] || fail 'Invalid display name'
    if [[ $first == true ]];then
        if [[ ! -f $OUTPUT/operation.json ]] && admin -o json auth doctor 2>/dev/null | grep -E '^  "identity":[[:space:]]*[{]' >/dev/null;then fail 'operator action expects the first bootstrap administrator; import an existing OP or use user --role super_admin';fi
        ROLE=super_admin
    else validate_scope;admin auth can-i access >/dev/null;fi
    [[ $OUTPUT == /* ]] || fail 'Use an absolute credential output directory'
    plan=$OUTPUT.init-plan.tsv
    text=$(printf 'action\t%s\nname\t%s\ncontext\t%s\nrole\t%s\nworkflow\t%s\nall_workflows\t%s\nvalid_for\t%s\nauthority_sha256\t%s\n' "$ACTION" "$NAME" "$CONTEXT_NAME" "$ROLE" "$WORKFLOW" "$ALL_WORKFLOWS" "$VALID_FOR" "$(sha "$AUTHORITY/ca.crt")";if ((${#PRESETS[@]}));then printf 'preset\t%s\n' "${PRESETS[@]}";fi)
    record_plan "$plan" "$text"
    if [[ -f $OUTPUT/operation.json ]];then
        admin resume credentials --output "$OUTPUT" >/dev/null
    else
        [[ ! -e $OUTPUT ]] || fail 'Partial credential material is retained; inspect it before retrying'
        local args=(create identities --authority "$AUTHORITY" --name "$NAME" --output "$OUTPUT" --valid-for "$VALID_FOR")
        [[ $first == false ]] || args+=(--super-admin)
        admin "${args[@]}" >/dev/null
    fi
    # credential.json is the native, public metadata record, not the private config.
    identity=$(sed -n 's/^[[:space:]]*"identity_uid":[[:space:]]*"\([A-Za-z0-9_.-]*\)".*/\1/p' "$OUTPUT/credential.json")
    identifier "$identity"
    if [[ $first == false ]];then
        binding=context-$(printf '%s' "$identity/$CONTEXT_NAME" | sha256sum | cut -c 1-40)
        local args=(auth grant "$binding" --person "$identity" --role "$ROLE")
        [[ -z $WORKFLOW ]] || args+=(--workflow "$WORKFLOW")
        local preset;for preset in "${PRESETS[@]}";do args+=(--preset "$preset");done
        admin "${args[@]}" >/dev/null
    fi
    FILE=$OUTPUT/config.json;SOURCE_CONTEXT=''
    import_context
    log "Personal credentials: $OUTPUT; role: $ROLE. Deliver only this recipient's config."
}
node_delivery(){
    local text file source_dir=${INIT_SCRIPT%/*} installer binary_sha installer_sha
    require_admin;admin auth can-i platform >/dev/null
    [[ -n $NODE && -n $OUTPUT ]] || fail 'Node initialization requires --node and --output DIR'
    identifier "$NODE"
    [[ $TTL =~ ^[0-9]+$ && $TTL -ge 60 && $TTL -le 3600 ]] || fail 'Bootstrap lifetime must be 60..3600 seconds'
    installer=$source_dir/install.sh
    [[ -f $installer && ! -L $installer ]] || fail 'Keep init.sh beside install.sh'
    [[ -z $HARDWARE_FILE || $HARDWARE_FILE =~ ^/[A-Za-z0-9_./-]+$ ]] || fail 'Hardware identity selector must be an absolute host path'
    binary_sha=$(sha "$BINARY");installer_sha=$(sha "$installer")
    text=$(printf 'action\tnode\nnode\t%s\nttl\t%s\nbinary_sha256\t%s\ninstaller_sha256\t%s\nhardware_id_file\t%s\n' "$NODE" "$TTL" "$binary_sha" "$installer_sha" "$HARDWARE_FILE")
    record_plan "$OUTPUT.init-plan.tsv" "$text"
    if [[ -e $OUTPUT ]];then [[ $RESUME == true ]] || fail 'Node delivery directory exists; no replacement bootstrap issued'
    else private_directory "$OUTPUT";fi
    private_directory "$OUTPUT"
    if [[ $(readlink -m -- "$BINARY") != "$OUTPUT/converyn" ]];then cp --reflink=auto -- "$BINARY" "$OUTPUT/converyn";fi
    chmod 700 "$OUTPUT/converyn"
    [[ $(sha "$OUTPUT/converyn") == "$binary_sha" ]] || fail 'Binary changed while preparing delivery'
    cp -- "$installer" "$OUTPUT/install.sh";chmod 700 "$OUTPUT/install.sh"
    [[ $(sha "$OUTPUT/install.sh") == "$installer_sha" ]] || fail 'Installer changed while preparing delivery'
    BINARY=$OUTPUT/converyn
    if [[ ! -f $OUTPUT/bootstrap.json ]];then admin admin bootstrap --node "$NODE" --ttl-seconds "$TTL" --output "$OUTPUT/bootstrap.json" >/dev/null;fi
    {
        printf 'Run on the target Agent host after privately transferring this directory.\n'
        printf 'From this private directory: sha256sum --check checksums.sha256\n'
        printf 'sudo bash ./install.sh agent --binary ./converyn --sha256 %q --bootstrap ./bootstrap.json --yes --non-interactive' "$(sha "$OUTPUT/converyn")"
        [[ -z $HARDWARE_FILE ]] || printf ' --hardware-id-file %q' "$HARDWARE_FILE"
        printf '\nKeep existing Agent identities/state. A used or expired bootstrap requires inspection, not a new Node UID.\n'
    } > "$OUTPUT/INSTALL.txt"
    (cd "$OUTPUT";sha256sum bootstrap.json converyn install.sh INSTALL.txt) > "$OUTPUT/checksums.sha256"
    log "Node delivery: $OUTPUT (one-use bootstrap, ${TTL}s). No OP credentials or issuing CA included."
}
validate_node_transport(){
    local option
    if [[ -z $NODE_SSH_HOST ]];then
        for option in --ssh-user --ssh-key --ssh-known-hosts --ssh-port --ssh-auth;do
            [[ ! ${SUPPLIED[$option]+present} ]] || fail "$option requires --ssh-host"
        done
        return
    fi
    [[ -n $NODE_SSH_USER ]] || fail '--ssh-host requires --ssh-user'
    [[ $NODE_SSH_PORT =~ ^[0-9]{1,5}$ ]] && ((10#$NODE_SSH_PORT>=1 && 10#$NODE_SSH_PORT<=65535)) || fail 'Invalid SSH port'
    case $NODE_SSH_AUTH in auto|key|password) ;;*) fail 'SSH authentication must be auto, key or password';;esac
    [[ $NODE_SSH_AUTH != password || ( $TTY == true && $NONINTERACTIVE == false && -z $NODE_SSH_KEY ) ]] || fail 'Password SSH requires a terminal and no --ssh-key'
    [[ -z $NODE_SSH_KEY || -f $NODE_SSH_KEY ]] || fail 'SSH key selector is not a file'
    (
        source "${INIT_SCRIPT%/*}/install.sh"
        validate_target "$NODE_SSH_USER@$NODE_SSH_HOST"
    ) || fail 'Invalid SSH host/user'
}
node_enrollment(){
    if [[ -z $NODE_SSH_HOST ]];then node_delivery;return;fi
    # Bind a delivery to its target before issuing bootstrap material. Authentication
    # choices may change on resume; the destination and Node identity may not.
    record_plan "$OUTPUT.ssh-plan.tsv" "$(printf 'node\t%s\nhost\t%s\nuser\t%s\nport\t%s\n' "$NODE" "$NODE_SSH_HOST" "$NODE_SSH_USER" "$NODE_SSH_PORT")"
    local node_initializer=$INIT_SCRIPT node_binary=$BINARY node_output=$OUTPUT node_uid=$NODE
    local node_config=$ADMIN_CONFIG node_context=$ADMIN_CONTEXT node_tty=$TTY node_noninteractive=$NONINTERACTIVE
    local node_key=$NODE_SSH_KEY node_hosts=$NODE_SSH_KNOWN_HOSTS node_port=$NODE_SSH_PORT node_auth=$NODE_SSH_AUTH node_hardware=$HARDWARE_FILE
    local node_target=$NODE_SSH_USER@$NODE_SSH_HOST
    local -a node_delivery_args=(node --binary "$BINARY" --admin-config "$ADMIN_CONFIG" --node "$NODE" --output "$OUTPUT" --ttl-seconds "$TTL" --yes --non-interactive)
    [[ -z $ADMIN_CONTEXT ]] || node_delivery_args+=(--admin-context "$ADMIN_CONTEXT")
    [[ -z $HARDWARE_FILE ]] || node_delivery_args+=(--hardware-id-file "$HARDWARE_FILE")
    [[ $RESUME == false ]] || node_delivery_args+=(--resume)
    (
        # Installer globals and traps stay in this subshell. One SSH control socket
        # serves preflight and install, including password/sudo authentication.
        source "${node_initializer%/*}/install.sh"
        BINARY=$node_binary DIRECTORY=$node_output SSH_AUTH=$node_auth
        TTY=$node_tty NONINTERACTIVE=$node_noninteractive SSH_KEY=$node_key SSH_PORT=$node_port KNOWN_HOSTS=$node_hosts
        HARDWARE_FILE=$node_hardware
        trap ssh_cleanup EXIT
        prepare_known_hosts
        ssh_arguments
        remote_preflight "$node_target"
        bash "$node_initializer" "${node_delivery_args[@]}" >&2
        INSTALL_SCRIPT=$node_output/install.sh BINARY=$node_output/converyn
        remote_install "$node_target" agent "$node_output/bootstrap.json" "$node_output/ssh.remote"
        wait_for "Agent $node_uid fresh authenticated heartbeat" agent_observed "$node_uid" "$node_config" "$node_context"
    )
}
browser_delivery(){
    [[ -n $FILE && -n $OUTPUT ]] || fail 'Browser export requires --file personal-config and --output DIR'
    private_file "$FILE"
    if [[ ( $NONINTERACTIVE == true || $TTY == false ) && -z $PASSWORD_FILE ]];then fail 'Noninteractive browser export requires --password-file';fi
    [[ -z $PASSWORD_FILE ]] || private_file "$PASSWORD_FILE"
    local args=(auth gui-login --output-dir "$OUTPUT" --with-credentials)
    [[ -z $PASSWORD_FILE ]] || args+=(--password-file "$PASSWORD_FILE")
    input "${args[@]}" >/dev/null
    log "Browser delivery: $OUTPUT (client.p12, token.txt, CA and recipient instructions)"
}
wizard(){
    local choice scope preset
    if [[ -z $INSTALLATION && -z $ADMIN_CONFIG && -z $FILE ]];then ask INSTALLATION 'Installation record directory / 安装记录目录' /secure/converyn-install/qa;fi
    ask choice '初始化 / Action: 1=导入已有OP 2=手工安装的首个OP 3=创建个人证书/context 4=导入个人配置 5=准备新增节点 6=浏览器证书' 1
    case $choice in
        1) ACTION=import;ask CONTEXT_NAME 'Context alias' qa-op;USE=true;;
        2) ACTION=operator;ask NAME 'First administrator display name' OP;ask CONTEXT_NAME 'Context alias' qa-op;ask OUTPUT 'New private credential directory' /secure/converyn-users/qa-op;USE=true;;
        3)
            ACTION=user;ask NAME 'Display name / 姓名' Alice;ask CONTEXT_NAME 'Context alias' qa-alice
            ask ROLE 'Role: viewer/executor/maintainer/platform_operator/access_admin/super_admin' executor
            case $ROLE in viewer|executor|maintainer)
                ask WORKFLOW 'Workflow ID (use * for all workflows)' demo
                if [[ $WORKFLOW == '*' ]];then WORKFLOW='';ALL_WORKFLOWS=true;fi
                if [[ $ROLE != maintainer ]];then ask preset 'Preset (empty means all presets)' qa;[[ -z $preset ]] || PRESETS+=("$preset");fi;;
            esac
            ask OUTPUT 'New private credential directory' "/secure/converyn-users/$CONTEXT_NAME";;
        4) ACTION=import;ask FILE 'Personal config file';ask CONTEXT_NAME 'Context alias' qa-alice;;
        5)
            ACTION=node;ask NODE 'New Node UID' node-a
            ask OUTPUT 'Private node delivery directory' "${INSTALLATION:-/secure/converyn}/nodes/$NODE"
            ask NODE_SSH_HOST 'SSH host (empty prepares delivery only)'
            if [[ -n $NODE_SSH_HOST ]];then
                ask NODE_SSH_USER 'SSH user' "${USER:-}"
                ask NODE_SSH_AUTH 'SSH authentication: auto/key/password' auto
                ask NODE_SSH_KEY 'SSH key (empty uses agent or password)'
                ask NODE_SSH_PORT 'SSH port' 22
            fi;;
        6) ACTION=browser;ask FILE 'Recipient personal config';ask OUTPUT 'New browser delivery directory' /secure/converyn-users/browser-login;;
        *) fail 'Unknown initialization action';;
    esac
}
parse(){
    if [[ ${1:-} =~ ^(import|operator|user|node|browser)$ ]];then ACTION=$1;shift;fi
    while (($#));do
        [[ $1 != --* ]] || SUPPLIED[$1]=1
        case $1 in
            --binary|--installation|--admin-config|--admin-context|--authority|--contexts|--context-name|--file|--source-context|--output|--name|--role|--workflow|--preset|--valid-for|--node|--hardware-id-file|--password-file|--ttl-seconds|--ssh-host|--ssh-user|--ssh-key|--ssh-known-hosts|--ssh-port|--ssh-auth)
                [[ $# -ge 2 && -n $2 && $2 != --* ]] || fail "Missing value for $1"
                case $1 in
                    --binary) BINARY=$2;;--installation) INSTALLATION=$2;;--admin-config) ADMIN_CONFIG=$2;;--admin-context) ADMIN_CONTEXT=$2;;--authority) AUTHORITY=$2;;
                    --contexts) CONTEXTS=$2;;--context-name) CONTEXT_NAME=$2;;--file) FILE=$2;;--source-context) SOURCE_CONTEXT=$2;;--output) OUTPUT=$2;;--name) NAME=$2;;--role) ROLE=$2;;
                    --workflow) WORKFLOW=$2;;--preset) PRESETS+=("$2");;--valid-for) VALID_FOR=$2;;--node) NODE=$2;;--hardware-id-file) HARDWARE_FILE=$2;;--password-file) PASSWORD_FILE=$2;;--ttl-seconds) TTL=$2;;--ssh-host) NODE_SSH_HOST=$2;;--ssh-user) NODE_SSH_USER=$2;;--ssh-key) NODE_SSH_KEY=$2;;--ssh-known-hosts) NODE_SSH_KNOWN_HOSTS=$2;;--ssh-port) NODE_SSH_PORT=$2;;--ssh-auth) NODE_SSH_AUTH=$2;;
                esac;shift;;
            --all-workflows) ALL_WORKFLOWS=true;;--use) USE=true;;--replace-context) REPLACE=true;;--resume) RESUME=true;;--yes) YES=true;;--non-interactive) NONINTERACTIVE=true;;
            --help|-h) usage;exit 0;;*) fail "Unknown option: $1";;
        esac;shift
    done
}
validate_options(){
    local option allowed
    for option in "${!SUPPLIED[@]}";do
        allowed=false
        case $option in
            --binary|--yes|--non-interactive|--help) allowed=true;;
            --installation) [[ $ACTION != browser ]] && allowed=true;;
            --admin-config|--admin-context) [[ $ACTION == operator || $ACTION == user || $ACTION == node ]] && allowed=true;;
            --authority|--name|--valid-for) [[ $ACTION == operator || $ACTION == user ]] && allowed=true;;
            --contexts|--context-name|--use|--replace-context) [[ $ACTION == import || $ACTION == operator || $ACTION == user ]] && allowed=true;;
            --file|--source-context) [[ $ACTION == import || $ACTION == browser ]] && allowed=true;;
            --output) [[ $ACTION != import ]] && allowed=true;;
            --role|--workflow|--preset|--all-workflows) [[ $ACTION == user ]] && allowed=true;;
            --node|--hardware-id-file|--ttl-seconds|--ssh-host|--ssh-user|--ssh-key|--ssh-known-hosts|--ssh-port|--ssh-auth) [[ $ACTION == node ]] && allowed=true;;
            --password-file) [[ $ACTION == browser ]] && allowed=true;;
            --resume) [[ $ACTION == operator || $ACTION == user || $ACTION == node ]] && allowed=true;;
        esac
        [[ $allowed == true ]] || fail "Option $option is not applicable to $ACTION"
    done
}
prepare_inputs(){
    case $ACTION in
        import|browser)
            [[ -n $FILE ]] || ask FILE 'Personal config file'
            private_file "$FILE";;
        operator|user|node) require_admin;;
    esac
    case $ACTION in
        import|operator|user)
            [[ -n $CONTEXT_NAME ]] || ask CONTEXT_NAME 'Context alias'
            identifier "$CONTEXT_NAME";;
    esac
    case $ACTION in
        operator|user)
            require_authority
            [[ -n $NAME ]] || ask NAME 'Display name'
            [[ $NAME != *[[:cntrl:]]* && $NAME != -* && -n ${NAME//[[:space:]]/} ]] || fail 'Invalid display name';;
        node)
            [[ -n $NODE ]] || ask NODE 'New Node UID'
            identifier "$NODE"
            [[ $TTL =~ ^[0-9]{1,4}$ ]] && ((10#$TTL>=60 && 10#$TTL<=3600)) || fail 'Bootstrap TTL must be 60..3600 seconds'
            [[ -z $HARDWARE_FILE || $HARDWARE_FILE =~ ^/[A-Za-z0-9_./-]+$ ]] || fail 'Hardware identity selector must be an absolute host path';;
    esac
    if [[ $ACTION == node && -z $OUTPUT && -n $INSTALLATION ]];then OUTPUT=$INSTALLATION/nodes/$NODE;fi
    if [[ $ACTION != import ]];then
        [[ -n $OUTPUT ]] || ask OUTPUT 'New private output directory'
        [[ $OUTPUT =~ ^/[A-Za-z0-9_./-]+$ && $OUTPUT != / && $OUTPUT != */../* && $OUTPUT != */./* && $OUTPUT != */.. && $OUTPUT != */. && $OUTPUT != *//* ]] || fail 'Use a normalized absolute private directory without spaces'
    fi
    if [[ $ACTION == node ]];then validate_node_transport;fi
    if [[ $ACTION == browser ]];then
        [[ -z $PASSWORD_FILE ]] || private_file "$PASSWORD_FILE"
        [[ $TTY == true && $NONINTERACTIVE == false || -n $PASSWORD_FILE ]] || fail 'Noninteractive browser export requires --password-file'
    fi
}
summary(){
    printf 'Action: init %s\nResult: succeeded\n' "$ACTION"
    [[ -z $INSTALLATION ]] || printf 'Installation: %s\n' "$INSTALLATION"
    [[ -z $OUTPUT ]] || printf 'Output: %s\n' "$OUTPUT"
    case $ACTION in
        import|operator|user) printf 'Config: %s\nContext: %s\nNext: converyn --config %q --context %q auth status\n' "$CONTEXTS" "$CONTEXT_NAME" "$CONTEXTS" "$CONTEXT_NAME";;
        node)
            if [[ -n $NODE_SSH_HOST ]];then printf 'Node: %s\nHeartbeat: verified\nNext: add the Node to the desired platform node group\n' "$NODE"
            else printf 'Next: follow %s/INSTALL.txt; verify the authenticated Node heartbeat after installation\n' "$OUTPUT";fi;;
        browser) printf 'Next: follow %s/README.md\n' "$OUTPUT";;
    esac
}
main(){
    [[ ${BASH_VERSINFO[0]} -gt 4 || ( ${BASH_VERSINFO[0]} -eq 4 && ${BASH_VERSINFO[1]} -ge 4 ) ]] || fail 'Bash 4.4 or newer is required'
    [[ ! -t 0 || ! -t 2 ]] || { exec 3<>/dev/tty;TTY=true; }
    FAILURE_CODE=2
    parse "$@";trap cleanup EXIT
    if [[ $TTY == false ]];then
        [[ $YES == true ]] || fail 'A terminal or --yes is required; use --yes --non-interactive for unattended execution'
    fi
    for command in readlink stat mkdir sha256sum cut cmp mktemp sed grep flock cp chmod;do need "$command";done
    [[ $ACTION != wizard ]] || wizard
    validate_options
    select_inputs
    if [[ $ACTION == user ]];then validate_scope;fi
    if [[ $ACTION == operator ]];then ROLE=super_admin;validate_scope;fi
    prepare_inputs
    log "Action: $ACTION; contexts: $CONTEXTS; output: ${OUTPUT:-existing personal config}"
    confirm
    FAILURE_CODE=1
    STAGE=$ACTION
    case $ACTION in import) import_context;;operator) issue_user true;;user) issue_user false;;node) node_enrollment;;browser) browser_delivery;;esac >&2
    summary
}
if [[ ${BASH_SOURCE[0]} == "$0" ]];then main "$@";fi
