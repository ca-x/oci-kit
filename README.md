# oci-kit

用于制作 OCI 镜像的 Rust 工具集。**通用机制不包含发行版配方；平台适配必须明确标注平台。**

| 二进制 | 范围 |
| --- | --- |
| `rootfs-fetch` | 通用：URL + SHA-256 → 校验后的原子缓存；不负责发现发行版的最新版 |
| `rootfs-inspect` | 通用：只读检查 tar / tar.gz / tar.zst，不解包、不执行内容 |
| `rootfs-cleaner` | 通用：按外部策略清理离线 staging rootfs，默认只输出计划 |
| `lighos-init` | **LightOS 专用**：额外 Arch keyring 恢复、平台服务初始化顺序 |

`lighos-init` 按用户指定的拼写命名，不提供名为 `rootfs-init` 的命令。
未经跨平台实现与验证，不把 LightOS 初始化包装成“通用 rootfs 初始化”。
Pond 最新版本解析、清理清单和 Dockerfile 属于独立的 `dockers-x/pond-oci` 配方仓库。

## rootfs 与通用性的边界

- **rootfs**：系统的根文件树，包含文件内容、owner、mode、链接及扩展元数据；不是统一的发行版配置协议。
- **OCI image**：根文件树的分层封装及启动配置；相同的封装不代表相同的内核接口、用户空间或初始化方式。
- **配方**：描述某个发行版的来源、清理取舍和启动策略；包管理器、libc、init、账号及桌面约定不能照搬。
- **平台 adapter**：对接目标平台的参数和生命周期；LightOS 的 hook 不因此适用于其他平台。

这里的“通用”指不内置发行版规则，**不是已支持任意操作系统**。
当前 workspace 面向 Linux，未声明 Windows、macOS 或所有 Linux 发行版的运行兼容性。

## 构建与验证

本项目的下载、依赖安装、编译与运行验收统一在 **GitHub Actions** 完成；
本地只编辑和审阅源码、通过 SSH 推送以及查看工作流状态，不下载 rootfs 或构建产物。
这是项目执行策略，不是通用 CLI 对运行环境的限制。
推送会触发全部验证，也可以只触发远程验证：

```sh
gh workflow run ci.yml --repo ca-x/oci-kit --ref main
gh run list --repo ca-x/oci-kit --workflow ci.yml
```

Linux，最低 Rust 1.89；stable 及最低版本都纳入验证。C 编译器用于依赖的构建。
离线 staging 操作要求 Linux 5.6+、可用的 `openat2` 和 `/proc`；不提供降级到不安全路径解析的 fallback。
下列构建、测试和各 CLI 示例用于 Actions runner 或其中的隔离环境，不在本机执行：

```sh
cargo build --locked --release --workspace --bins
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo doc --locked --workspace --no-deps
```

产物在 `target/release/`。将工具复制到其他发行版前，检查 libc 兼容性；
镜像配方使用 Alpine Rust builder 编译 musl 静态版本，不能直接假设本机 GNU 产物可运行。
CI 分别验证 stable、Rust 1.89 和 Linux x86-64 musl release：检查 ELF 无动态加载器/共享库依赖，
执行真实 CLI 测试，并在临时 root 中验收初始化。静态构建命令、逐功能证据与边界见
`docs/cli-acceptance.md`；不能把这套工具验收等同于镜像或桌面启动验收。

## rootfs-fetch

最小接口只识别下载契约，不知道 Pond、Arch、LightOS 或 GitHub 的发布格式：

```sh
rootfs-fetch --url https://example.org/rootfs.tar.zst \
  --sha256 "$EXPECTED_SHA256" --output ./cache/rootfs.tar.zst
rootfs-fetch --source rootfs.lock.json --output ./cache/rootfs.tar.zst
```

source 文档：

```json
{
  "schema_version": 1,
  "url": "https://example.org/rootfs.tar.zst",
  "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
  "metadata": { "revision": "upstream-immutable-revision" }
}
```

`size` 是可选字节数，`metadata` 是不透明的字符串映射。示例摘要只是格式示范，
不是可下载工件的真实摘要。工具要求 SHA-256，默认仅 HTTPS，包括重定向；
`--allow-http` 仅用于明确可信的测试网络。
已有文件会重新校验，错误文件不会被悄悄覆盖。成功后写入 `.source.json` 旁车文件。
下载失败不会留下可误认成功的目标文件；不支持断点续传。
摘要保证内容一致性，不替代上游身份或签名验证。source/旁车会记录完整 URL，
不要把长期凭据嵌入 URL 或 metadata。

## rootfs-inspect

```sh
rootfs-inspect --archive rootfs.tar.zst --select usr/share/wayland-sessions \
  --text etc/os-release > report.json
```

支持重复 `--select` 查看路径及子树元数据；`--text` 只读取最多 64 KiB 的普通 UTF-8 文件。
拒绝绝对路径与 `..` 路径。该命令不解包，不能代替解包工具的安全检查。
不支持的压缩格式应先通过可信工具转换。设备、链接和数字 owner 只作报告。

## rootfs-cleaner

只处理**未挂载、不运行任何进程、由当前构建独占**的 staging 目录。
不要给它运行中的系统目录。先独立确认 staging，再计划、再显式执行：

```sh
rootfs-cleaner mark --root ./staging
rootfs-cleaner clean --root ./staging --policy cleanup.toml
rootfs-cleaner clean --root ./staging --policy cleanup.toml --apply
```

```toml
schema_version = 1

[[remove]]
path = "var/cache/example"
reason = "Downloaded artifacts are not runtime dependencies"
```

路径是相对 rootfs 的**精确路径**，不是 glob、包名或 shell 表达式。
拒绝 `/`、父路径穿越、symlink ancestor、挂载子树及 staging marker；
删除 symlink leaf 只删除链接，不追随目标。文件操作使用 capability-relative 目录句柄。
先预检全部规则，再开始删除；执行不是跨文件事务，I/O 错误可能留下部分清理结果，
应从可丢弃 staging 重新构建，不能把它当成回滚工具。

保留文件的 owner、ACL、xattr 等不被重写。**清理必须发生在生成最终 OCI layer 之前**，
在已有层上 `rm` 只会隐藏旧内容，不会减少旧层字节。工具不假装卸载软件包，
不运行 package hooks，也不会自动删除内核、firmware、Nix 或 Flatpak。

## lighos-init

这是 `crates/lighos-init` 中的 LightOS adapter，不是通用初始化引擎。

构建期需要先安装 Arch 软件包时，使用显式离线入口，而不是伪造平台初始化成功：

```sh
lighos-init prepare-keyrings --root ./staging --config lightos.toml
lighos-init prepare-keyrings --root ./staging --config lightos.toml --apply
```

默认只输出 JSON 计划；应用需要 root 和构建宿主的 `/usr/bin/bwrap`。
目标必须已由 `rootfs-cleaner mark` 标记且不是运行根，不能有平台 seed、完成标记
或远程认证文件。入口只检查这些状态的 metadata，不读取内容。
先检查全部公开 keyring，再在独立 PID/IPC/network/UTS/mount namespace 内调用
原生 `pacman-key --init` 和 add/populate；固定目录 fd，清空父子命令环境，不联网。
基础 `archlinux` 与声明的额外 keyring 各处理一次。失败立即返回，不创建用户、认证、
服务启用状态或 kit/平台完成标记；原有运行期 `apply` 的前置条件保持不变。
配方必须在包事务后清理临时生成的 GPG 私钥/信任状态，再由平台和 post-setup hook
在每个实例重建；这个入口不会把准备过的构建目录当作已初始化的实例。
该入口在代码 `eea666f` 的 GitHub `40809926162` 已通过 Rust/MSRV、fmt/clippy/doc、
静态执行与全部 26 个运行用例，独立窄审无新增 P1/P2；受控 helper 验收不表示
Pond 的真实 GPG/仓库签名和包事务已通过。

接口支持非秘密的 `pacman_keyrings`、可选 `post_setup_services`，以及显式设备授权
`render_node_services`（新增 generator 的远程验收状态见 `docs/cli-acceptance.md`）：

```toml
schema_version = 1
pacman_keyrings = ["extra-repository-keyring"]
post_setup_services = [
    "lightos-remote-desktop.service",
    "lightos-remote-desktop-session.service",
]
render_node_services = [
    "lightos-remote-desktop.service",
    "lightos-remote-desktop-session.service",
]
```

```sh
lighos-init install --root ./staging --config lightos.toml
lighos-init apply --config /etc/oci-kit/lightos.toml
```

`install` 仅把自身、配置和 systemd unit 放入已标记 staging，不触及宿主服务。
可选服务列表只添加 kit 命名空间的 `Requires/After=lighos-init.service` drop-in，
不替换、启用或启动原服务，也不读取远程认证 env。平台完成标记使用 Assert，缺失时失败，
不能把跳过 Condition 当作初始化成功。列表最多 32 个具体 service，拒绝前缀、模板和循环名称。
配置重装只移除前一配置中已删除且内容完全匹配的 kit drop-in，保留其他配置与目录；
修改过的文件、symlink 和特殊文件会阻止更新。安装配置最多 64 KiB，协调成功后才替换。
`render_node_services` 默认空，必须是初始化排序列表的无重复子集；它授权给列出的
service 进程，而非猜测平台用户名。安装同时放置 `lighos-init-generator` symlink，
仍调用同一个 Rust 可执行文件，不使用额外初始化 shell。
systemd 在启动/reload 时读取已映射 DRM render 字符节点的数字 GID，在 normal
generator 目录生成 additive `SupplementaryGroups=` drop-in，不改账号、group 数据库、
设备权限或服务启用状态。没有 render 节点时不生成规则，允许软件渲染的图形桌面。
数字 group 授权作用于该 GID 可访问的全部资源，不是单设备 ACL；只对明确选中的服务启用。
不从 card/input/TTY 节点发现 GID；组的实际权限由平台映射决定。
不跟随节点/输出路径链接，不接受 root/无效 GID。
平台在生成之后才映射设备时需要重新加载；已运行进程不会凭空获得新 group，不能因此
擅自重启持久桌面。生成文件由 systemd 清除并重建，不写 kit 成功状态或平台认证。
`apply` 默认仍是计划；实际首次启动的 unit 添加 `--apply`。
它要求 root 身份和 `/var/lib/lightos/initialized`，在 `lightos-setup.service` 之后运行，
不替换平台注入脚本，不读取 seed、不创建重复账号、不接管 SSH。
成功状态保存由工具版本与序列化配置计算的指纹；失败不写成功状态；相同配置重启跳过。
运行时文件锁阻止并发初始化。需要手动恢复 keyring 时，应先处理失败原因，
再由管理员删除 kit 自身的完成状态；不要删除 LightOS 的账号完成状态来冒险重建账号。

`apply` 的 stdout 始终是单个 JSON 文档；显式应用成功后增加 `changed`，失败不输出成功文档。
子命令输出走 stderr。先检查全部 keyring 文件，再逐个 `pacman-key --add` 并 `--populate`，
避免 populate 内部导入失败但仍返回零时错误写入成功状态。
运行时状态目录及其祖先须由 root 控制且不能被普通用户写入；拒绝 symlink、FIFO、设备等状态/锁文件。
锁用于协调工具实例，不防御拥有 root 权限的进程恶意替换状态目录或文件。

**尚未声明完成：**通用账号/locale 配置、其他平台 adapter、实际进程/GPU 设备验收、Pond/Niri 远程桌面、可选物理 VT/PAM/logind 验收。
工具测试通过不等于这些场景已经通过平台启动验证。

## 配方与协议

先确定 artifact → 安全 staging → 显式策略 → 平台 hook → 最终 rootfs layer。
发行版“latest”发现由配方输出通用 source 文档；平台参数协议属于平台，不属于 OCI 格式。
详见 `SPEC.md`。仓库不包含上游 rootfs，不自动获得任何第三方内容的再分发许可。
