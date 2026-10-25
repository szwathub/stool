# stool

开发环境管理工具：部署 dotfiles、初始化 macOS，并在仓库外加密备份和恢复 GPG / SSH 密钥。

| 命令 | 用途 |
|---|---|
| `stool init` | 保存仓库 URL、恢复密钥并准备 macOS 环境 |
| `stool dotfile` | 初始化、预览、部署和更新配置 |
| `stool apple` | macOS 偏好、Homebrew 和 Brewfile 安装 |
| `stool secrets` | 离线密钥备份与恢复 |

## 安装

macOS / Linux 通过发行包安装，无需 Rust、Git 或 Homebrew：

```bash
curl -fsSL https://raw.githubusercontent.com/szwathub/stool/master/install.sh | bash
export PATH="$HOME/.local/bin:$PATH"
stool --version
```

安装器校验 SHA-256，默认安装到 `~/.local/bin`。Windows 从 [Releases](https://github.com/szwathub/stool/releases) 下载 ZIP，核对 `SHA256SUMS.txt` 后解压。

## 新机器流程

以下以 macOS 和私有仓库 `szwathub/dotfiles` 为例。先在旧机器完成[密钥备份](#密钥备份)，再手动将加密包转入新机器；仓库、路径和 SSH 文件名按实际情况替换。

### 1. 初始化

缺少 Git 时，先运行 `xcode-select --install`，等待 Command Line Tools 安装完成：

```bash
stool init --dotfile-url git@github.com:szwathub/dotfiles.git \
  --key-file /Volumes/Backup/my-keys.tar.age
```

`init` 保存 URL 并准备环境，不克隆或部署配置。缺少 GPG 且尚不能安装时先恢复 SSH，GPG 留到第 4 步。没有离线包可省略 `--key-file`。

### 2. SSH 认证

恢复的 SSH key 须已获 GitHub 授权，并有私库读取权限，无需额外 PAT。`dotfile init` 优先沿用现有 SSH 配置或 agent；遇到公钥认证失败时，会从 `~/.ssh` 中验证可访问目标仓库的身份并保存，通常无需手动执行 `ssh-add` 或设置 `GIT_SSH_COMMAND`。

多身份或加密私钥可在 `stool init` 或 `stool dotfile init` 中一次指定 `--ssh-key "$HOME/.ssh/github_rsa"`。`init` 在离线恢复后保存该路径；私钥原口令由 SSH 在终端中询问，与备份密码独立。

首次连接核对 [GitHub 主机指纹](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/githubs-ssh-key-fingerprints)。也可使用 HTTPS，先通过凭据助手保存有私库读取权限的 [PAT](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens)：

```bash
stool init --dotfile-url https://github.com/szwathub/dotfiles.git
git config --global credential.https://github.com.helper osxkeychain
git ls-remote https://github.com/szwathub/dotfiles.git HEAD
```

### 3. 部署 dotfiles

```bash
stool dotfile init
stool dotfile diff
stool dotfile apply
stool dotfile verify
```

`dotfile init` 默认读取已保存的 URL，并递归初始化子模块。先审阅 diff，再处理文件冲突；仅使用可信仓库。

### 4. 安装软件并补齐 GPG

示例仓库将 Brewfile 部署到 `~/.Brewfile`，其中包含 GPG Suite。确认清单可信后执行：

```bash
stool apple brew install --dry-run
stool apple brew install
# 按安装输出配置当前终端的 Homebrew shellenv，确保 gpg 位于 PATH
gpg --version
# 若初始化时尚未恢复 GPG，再执行：
stool secrets restore --gpg --input /Volumes/Backup/my-keys.tar.age
gpg --list-secret-keys --fingerprint
```

使用 GPG Suite 时无需另装 gnupg / pinentry-mac。最后确认个人与 Work 身份均可签名、验签。

## 常用命令

```bash
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
├── .stool.toml.tmpl                 # 初始化本机配置
├── .stooldata.toml                  # 模板数据
├── .stoolignore                     # 忽略规则
├── .stooltemplates/                 # 共享模板
├── dot_gitconfig.tmpl               # ~/.gitconfig
└── private_dot_ssh/private_config   # ~/.ssh/config
```

| 规则 | 用途 |
|---|---|
| `dot_`、`private_`、`executable_` | 隐藏名称和权限 |
| `.tmpl` | 渲染模板 |
| `empty_`、`external_` | 空文件和原样部署子目录 |
| `.stoolversion` | 最低 stool 版本 |
| `.stoolexternal.toml[.tmpl]` | 外部资源 |

模板使用 MiniJinja，如 `{{ stool.os }}`。数据依次由仓库、本机 `[data]`、`--data key=value` 覆盖；字符串使用内置 `git_quote()`、`sh_quote()`、`fish_quote()`，返回值包含引号。本机配置位于 `~/.stool/dotfile/config.toml`。

配置加密使用 `stool dotfile keygen --output <身份文件>` 生成身份文件，配置本机 `[encryption]`，再通过 `add --encrypt` 导入。密钥离线备份使用下方的 `stool secrets`。

## 密钥备份

在 Git 仓库外执行，默认备份全部可导出的 GPG 密钥、信任数据、撤销证书及 SSH 密钥对；可用 `--gpg` 或 `--ssh` 筛选：

```bash
stool secrets backup --output /Volumes/Backup/my-keys.tar.age
stool secrets restore --input /Volumes/Backup/my-keys.tar.age
```

备份密码在终端隐藏输入，单独保管。备份包、恢复目录和密码文件须位于 Git 工作区外；恢复默认写入 HOME，不覆盖冲突文件。仅存于硬件或 agent 中的私钥不在备份范围内。

MIT · Zhiwei Sun (@szwathub) · szwathub@gmail.com
