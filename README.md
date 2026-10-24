# stool

开发环境管理工具：从 Git 部署 dotfiles、初始化 macOS，并在仓库外备份和恢复 GPG / SSH 密钥。

| 命令 | 用途 |
|---|---|
| `stool init` | 保存默认仓库 URL，按需恢复密钥并准备 macOS 环境 |
| `stool dotfile` | 初始化、预览、部署和更新配置 |
| `stool apple` | macOS 偏好、Homebrew 和 Brewfile 安装 |
| `stool secrets` | 密码加密的离线密钥备份与恢复 |

[安装](#安装) · [新机器流程](#新机器流程) · [常用命令](#常用命令) · [配置仓库](#配置仓库) · [密钥备份](#密钥备份) · [开发与发布](#开发与发布)

## 安装

macOS / Linux 通过发行包安装，无需 Rust、Git 或 Homebrew：

```bash
curl -fsSL https://raw.githubusercontent.com/szwathub/stool/master/install.sh | bash
export PATH="$HOME/.local/bin:$PATH"
stool --version
```

安装器校验 SHA-256，默认安装到 `~/.local/bin`，PATH 需自行持久化。Windows 从 [Releases](https://github.com/szwathub/stool/releases) 下载 ZIP，核对 `SHA256SUMS.txt` 后解压。

源码构建需要 Rust >= 1.74：

```bash
cargo build --locked --release
export PATH="$PWD/target/release:$PATH"
```

## 新机器流程

以下以新 Mac 和私有仓库 `szwathub/dotfiles` 为例，替换为自己的仓库、备份路径和 SSH 文件名。

### 1. 准备离线备份

在已有密钥的机器上，于 Git 仓库外执行，再手动将包转入新机器：

```bash
stool secrets backup --output /Volumes/Backup/my-keys.tar.age
```

备份范围见 [密钥备份](#密钥备份)。密码单独保存，转移前在隔离目录验证恢复。

### 2. 安装 stool 并初始化

macOS 缺少 Git / Command Line Tools 时，先运行 `xcode-select --install`，安装完成后继续：

```bash
stool init --dotfile-url git@github.com:szwathub/dotfiles.git \
  --key-file /Volumes/Backup/my-keys.tar.age
```

`init` 保存 URL、恢复密钥并准备 Homebrew 和 macOS 偏好，不克隆或部署配置。缺少 GPG 时先恢复 SSH；若有 `~/.Brewfile` 则安装后再检查 GPG，尚不可用时保留待恢复项。没有离线包可省略 `--key-file`。

### 3. 验证私库认证

已获 GitHub 授权的 SSH key 无需 PAT。部署配置前，先加载身份：

```bash
[ -n "${SSH_AUTH_SOCK:-}" ] || eval "$(/usr/bin/ssh-agent -s)"
/usr/bin/ssh-add "$HOME/.ssh/github_rsa"
git -c core.sshCommand='ssh -i ~/.ssh/github_rsa -o IdentitiesOnly=yes' \
  ls-remote git@github.com:szwathub/dotfiles.git HEAD
```

首次连接核对 [GitHub 主机指纹](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/githubs-ssh-key-fingerprints)；恢复私钥不会增加账号权限。

使用 HTTPS 时，改存 HTTPS URL，并在交互终端缓存凭据：

```bash
stool init --dotfile-url https://github.com/szwathub/dotfiles.git
git config --global credential.https://github.com.helper osxkeychain
git ls-remote https://github.com/szwathub/dotfiles.git HEAD
```

Password 输入有目标私库 Contents 读取权限的 [PAT](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens)，不要使用账号密码或将凭据放入 URL。系统 Git 后端不询问凭据，须先完成预检；其他系统使用本机凭据助手。

### 4. 部署 dotfiles

```bash
stool dotfile init
stool dotfile diff
stool dotfile apply
stool dotfile verify
```

`dotfile init` 默认读取已保存 URL，递归初始化子模块；`--no-recurse-submodules` 可关闭。先审阅 diff，再处理已有文件冲突；非交互覆盖需明确使用 `apply --force`。

### 5. 安装软件，补齐 GPG

示例仓库将包含 GPG Suite 的 Brewfile 部署到 `~/.Brewfile`，确认清单可信后安装：

```bash
stool apple brew install --dry-run
stool apple brew install
# 按输出配置当前终端的 Homebrew shellenv，再检查 gpg
gpg --version
# 仅在初始化时尚未恢复 GPG 的情况下执行
stool secrets restore --gpg --input /Volumes/Backup/my-keys.tar.age
gpg --list-secret-keys --fingerprint
```

确认个人与 Work 身份均可签名、验签。stool 复用 PATH 中的 `gpg`；GPG Suite 通常需要 `/usr/local/bin`，无需另装 gnupg / pinentry-mac。

`verify` 只检查配置；软件安装与密钥恢复独立于 `apply/update`。

## 常用命令

```bash
stool init --dry-run
stool dotfile status
stool dotfile update
stool dotfile add ~/.gitconfig ~/.config/fish
stool dotfile edit ~/.gitconfig --apply
stool dotfile doctor
stool apple init --dry-run
```

完整参数见 `stool <命令> --help`。

## 配置仓库

```text
dotfiles/
├── .stool.toml.tmpl                 # 初始化本机数据
├── .stooldata.toml                  # 仓库模板数据
├── .stoolignore                     # 忽略规则
├── .stooltemplates/                 # 共享模板
├── dot_gitconfig.tmpl               # ~/.gitconfig
├── dot_config/fish/config.fish      # ~/.config/fish/config.fish
└── private_dot_ssh/private_config   # 私有 SSH 配置
```

| 规则 | 用途 |
|---|---|
| `dot_`、`private_`、`executable_` | 隐藏名称、私有权限和 Git hook 执行权限 |
| `.tmpl` | 渲染 Git、Fish 等配置模板 |
| `empty_` | 部署空文件，当前用于 `gpg.conf` |
| `external_` | 原样部署 Fish 主题子模块的目录内容 |
| `.stoolversion` | 检查模板所需的最低 stool 版本 |
| `.stoolexternal.toml[.tmpl]` | 声明外部资源，当前用于可选的 RVM Fish 脚本下载 |

模板使用 MiniJinja，例如 `{{ stool.os }}`。数据优先级为仓库数据、本机 `[data]`、`--data key=value`。Git、Shell、Fish 字符串分别使用内置 `git_quote()`、`sh_quote()`、`fish_quote()`，返回值已包含引号。

配置文件加密先运行 `stool dotfile keygen --output <身份文件>`，设置本机 `[encryption]` 的 backend / recipients / identities，再用 `add --encrypt` 导入。GPG 后端调用 `gpg`。

本机部署配置位于 `~/.stool/dotfile/config.toml`。子模块和 LFS 需要系统 Git；update 只快进，拒绝脏仓库和分叉。

覆盖或删除前会备份，冲突须显式处理。仅使用可信仓库：即使预览，模板仍可调用外部程序；diff / archive 可能包含明文敏感内容。

## 密钥备份

密钥包独立于 dotfiles，存放于仓库外：

```bash
# 在仓库外运行；默认在当前目录生成带时间戳的新包
stool secrets backup

# 恢复目标默认 HOME
stool secrets restore --input /Volumes/Backup/my-keys.tar.age
```

默认处理全部密钥，可用 `--gpg` 或 `--ssh` 单独选择；仅处理 SSH 不依赖 Git 或 GPG。恢复目标默认 HOME，密码在终端隐藏输入。

包内包含全部可导出的 GPG 主 / 子私钥、对应公钥、ownertrust、撤销证书，以及 SSH 密钥对；不包含仅存于硬件或 agent 中的秘密、系统钥匙串或整个 GPG 数据库。

备份输出、恢复目标和密码文件须位于 Git 工作区外，不覆盖已有包或冲突文件。GPG 导入与多文件恢复无整体回滚，失败后修复重试；备份密码与原私钥口令分别保管。

## 开发与发布

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo +1.74.0 test --locked --all-targets
STOOL_BIN=target/debug/stool python3 -m unittest discover -s tests/python
sh scripts/generate-cli-assets.sh
```

维护脚本需要 Python >= 3.12，原生密钥测试另需 GPG / OpenSSH。修改 CLI 后重新生成补全和手册，CI 会校验。

### Git-flow

`develop` 为默认集成分支，`master` 为发布主线。

1. 功能使用 `feature/* → develop`；发布准备从 develop 创建 `release/<版本>`，紧急修复从 master 创建 `hotfix/<版本>`。
2. 更新 Cargo.toml、Cargo.lock 和 RELEASE_NOTES.md，通过 PR 和 Check，以 merge commit 合并到 master。
3. 等该合并提交的 master push Check 和 Master release gate 通过，再从同步后的 master 创建签名标签。
4. 发布并验收成功后，将 master 通过 PR 回合并 develop；hotfix 同步到正在准备的 release。

```bash
stool_release_version='X.Y.Z'   # 发布时替换为实际版本号
git switch master
git pull --ff-only origin master
python3 scripts/git-flow.py tag "v$stool_release_version" --push
```

正式标签只允许指向 master 第一父链上的 release / hotfix 合并提交。打标签前须登录 gh，并配置可被 GitHub 验证的 GPG 签名。

`.github/rulesets/` 提供分支和标签保护配置，需在 GitHub 手动应用。

MIT · Zhiwei Sun (@szwathub) · szwathub@gmail.com
