<div align="center">

# ChatBox

**面向 Web 与 Android 的实时多人聊天平台。**

[![状态](https://img.shields.io/badge/状态-设计%2F早期阶段-orange)](#-项目状态)
[![Web](https://img.shields.io/badge/Web-SvelteKit-FF3E00?logo=svelte&logoColor=white)](https://kit.svelte.dev/)
[![Android](https://img.shields.io/badge/Android-Kotlin-7F52FF?logo=kotlin&logoColor=white)](https://kotlinlang.org/)
[![后端](https://img.shields.io/badge/后端-Rust-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![数据库](https://img.shields.io/badge/数据库-MySQL-4479A1?logo=mysql&logoColor=white)](https://www.mysql.com/)
[![缓存](https://img.shields.io/badge/缓存-Redis-DC382D?logo=redis&logoColor=white)](https://redis.io/)
[![许可证](https://img.shields.io/badge/许可证-GPLv3-blue?logo=gnu&logoColor=white)](LICENSE)

[English](README.md) · 简体中文

</div>

---

## 📖 项目简介

**ChatBox** 是一款开放、可扩展的即时通讯平台，同时支持**单聊**与**群聊**。
它面向现代化的跨平台体验：**SvelteKit** Web 客户端、原生 **Kotlin** Android
客户端，以及高性能、可水平扩展的 **Rust** 后端。后端部署在 **MySQL** 与
**Redis** 集群之上。

项目从第一天起就以生产环境为标准进行设计——并发、超时、数据库锁、主从同步延迟、
故障切换、缓存一致性、缓存雪崩等问题，均被视为一等需求，而非事后补救。

> **默认语言为英文。** 所有代码注释、标识符、提交信息与文档均使用英文，以便国际社区
> 参与和阅读。

---

## 🚧 项目状态

ChatBox 目前处于**早期开发**阶段。后端（Rust）、Web 客户端（SvelteKit）与 Android
客户端（Kotlin）已实现本文档中的核心功能集：身份与会话、单聊与群聊、联系人、搜索、
拉黑名单以及双层插件系统。数据库结构亦支持主从读取与由 Redis 支撑的基础设施。
剩余工作见[路线图](#-路线图)。

在首个稳定版本发布前，接口与目录结构仍可能发生变化。

---

## ✨ 功能特性

### 💬 消息
- **单聊** —— 可发送**文字**或**文件**。
- **不提供在线预览** —— 所有附件均以可下载文件的形式发送，平台刻意不渲染文件预览。
- **离线消息** —— 消息会被存储，并在接收方重新上线时投递。
- **多设备同时登录** —— 同一账号可在多台设备上同时在线，消息在各端同步。
- **消息提醒** —— 在群聊中可通过 `@` 提及成员。

### 🔎 搜索
- **类型搜索** —— 仅针对文件，按文件名搜索，支持**通配符**（例如 `*.mp4`）。
- **时间搜索** —— 按时间范围过滤，精确**到分钟**。
- **内容搜索** —— 对文字消息进行全文检索。
- **排序** —— 搜索结果可按时间**升序或降序**排列。
- **跳转到消息** —— 点击搜索结果后返回会话页面，并自动滚动到该条消息并高亮显示。

### 👥 联系人与身份
- **ID 登录**，配合密码验证。
- **自定义 ID** —— 用户可自定义，带唯一性 / 冲突检查。
- **自定义用户名** —— 未设置时回退显示为用户 ID。
- **备注 / 昵称** —— 为每个联系人设置别名。
- **按 ID 添加联系人** —— 发送好友申请并可附上介绍；对方**同意**后才成为联系人。

### 🗣️ 群聊
群聊包含三种角色：**群主**、**管理员**、**普通成员**。

| 能力 | 群主 | 管理员 | 普通成员 |
|---|:---:|:---:|:---:|
| 发送消息（未被禁言时） | ✅ | ✅ | ✅ |
| 修改群名称 | ✅ | ✅ | ❌ |
| 限制普通成员拉人入群 | ✅ | ✅ | ❌ |
| 禁言成员 | ✅ | ✅ | ❌ |
| 踢人 / 拉人 | ✅ | ✅ | ❌ |
| 转让群主 | ✅ | ❌ | ❌ |
| 解散群聊 | ✅ | ❌ | ❌ |

- 每个群同时**仅有一位**群主。
- 群主拥有管理员的全部权限，**并额外拥有**转让群主与解散群聊的权限。

### 🚫 单聊管控
- 在一对一会话中**拉黑 / 解除拉黑**用户。

### 🔌 可扩展性

ChatBox 提供**插件系统**，可在**无需修改核心**的前提下扩展新能力。插件分为两类，
二者在安装位置与影响范围上完全不同：

- **客户端插件** —— 由**用户**在自己的设备上安装，安装后对该客户端**全局生效**，
  且仅影响本地客户端行为，**完全无需服务端做任何改动**。典型例子包括语言包（翻译）、
  UI 主题以及其他客户端功能。是否安装或卸载完全由用户自行决定。
- **服务端插件** —— 由**服务器管理员**在后端安装。服务端插件运行在服务端，**必须
  部署并在服务器上启用**后才会生效。客户端无法安装服务端插件，且服务端插件可能为
  所有已连接用户新增或改变行为。服务端插件可挂接消息投递、注册与建群流程：持久化前
  的钩子可以拒绝消息或改写内容，插件还可通过只读 Host API 读取用户/会话或保存插件
  私有状态。

简而言之：**客户端插件由用户安装、作用本地（在该客户端上全局生效，不影响服务端）；
服务端插件由管理员安装，只有存在于服务器上时才会生效。**

---

## 🛠️ 技术栈

| 层次 | 技术 | 说明 |
|---|---|---|
| Web 客户端 | **SvelteKit** | SSR + SPA，使用 WebSocket 实现实时通信 |
| Android 客户端 | **Kotlin** | 原生 Android |
| 后端 | **Rust** | 异步、内存安全、可水平扩展 |
| 主数据库 | **MySQL** | **必须**；支持集群（分片、主/从） |
| 缓存 / 发布订阅 | **Redis** | 支持集群（单节点或 Redis 集群）；可在配置中选择关闭 |

---

## 🏗️ 系统架构

ChatBox 将**控制面**（HTTP/REST）与**数据面**（WebSocket 实时通信）分离，两者独立
扩缩容，慢客户端永远不会阻塞请求/响应流量。应用节点保持无状态，所有共享状态置于
Redis、MySQL 与对象存储之中。

```mermaid
graph TD
    subgraph 客户端
        WEB[SvelteKit Web]
        AND[Kotlin Android]
    end

    WEB -->|HTTPS| LB[L7 API 网关]
    AND -->|HTTPS| LB
    WEB -->|WSS| WSLB[WebSocket 负载均衡]
    AND -->|WSS| WSLB

    LB --> API1[API 节点 1]
    LB --> APIN[API 节点 N]

    WSLB --> GW1[网关节点 1]
    WSLB --> GWN[网关节点 N]

    API1 & APIN & GW1 & GWN --> REDIS[(Redis 集群)]
    API1 & APIN --> MQ[(持久化消息流 / Broker)]
    MQ --> FAN[扇出 Worker]
    FAN --> GW1 & GWN
    GW1 & GWN --> MQ
    API1 & APIN & FAN --> PRIMARY[(MySQL 主库)]
    API1 & APIN & GW1 & GWN --> REPLICA[(MySQL 从库)]
    PRIMARY -->|异步复制| REPLICA
    API1 --> OBJ[(对象存储)]
```

**核心思路**

- **独立扩缩容的双平面。** HTTP API 与 WebSocket 网关分别扩容；连接数突增不会挤占
  REST 处理能力。
- **无状态节点 + 连接注册表。** 任意节点均可处理任意请求（JWT / Redis 会话）。在线
  连接记录在 Redis 中（`conn:{user_id} -> {(node, device, conn_id, last_seen)}`），
  配合 TTL 心跳，消息可精确路由到持有该连接的网关节点，而不是广播到所有节点。
- **发送与投递解耦。** 写入通过事务性 **outbox** 模式落库 MySQL，再发布到持久化
  消息流。扇出 Worker 负责投递；失败或缓慢的消费者不会阻塞发送方。
- **持久化、至少一次投递。** 每条消息携带客户端生成的幂等键（`client_msg_id`），
  接收端去重。"恰好一次"针对的是**副作用**，通过幂等写入实现，而非假设网络永不失败。
- **按会话定序。** 每个会话维护单调递增序列号，客户端据此排序、检测空洞并请求
  重同步，无需全局锁。
- **混合扇出。** 小群写入时扇出（推送）；超大群与频道读取时扇出（共享时间线 +
  每设备游标），避免写放大。在线成员收到推送提示，离线设备重连后按游标补齐。
- **写主库、读从库**，配合读己之写路由与显式的主从延迟感知。
- **Redis** 承载缓存、在线状态、连接注册表、限流计数与扇出传输（Streams）；分布式锁
  仅作为最后手段。
- **对象存储直传**：通过预签名 URL 上传大文件，避免流量经过 API 节点。

---

## 📁 仓库结构

```text
chatbox/
├── backend/            # Rust workspace（API、实时网关、后台任务）
│   ├── crates/
│   │   ├── api/        # HTTP/REST 处理器
│   │   ├── realtime/   # WebSocket 网关
│   │   ├── workers/    # 扇出、outbox 中继、后台任务
│   │   ├── core/       # 领域逻辑、服务
│   │   ├── storage/    # MySQL / Redis / 内存数据访问
│   │   ├── plugin/     # 插件运行时与接口
│   │   └── server/     # 组合根（二进制）
│   └── Cargo.toml
├── web/                # SvelteKit Web 客户端
├── android/            # Kotlin Android 客户端
├── deploy/             # Docker Compose、数据库迁移、环境示例
│   ├── docker/
│   └── sql/
├── docs/               # 设计文档与 ADR
├── scripts/            # 开发与冒烟测试脚本
└── README.md
```

---

## 🚀 快速开始

### 环境要求

| 工具 | 建议版本 |
|---|---|
| Rust | stable（2024 edition 或更新） |
| Node.js | 20 LTS 或更新 |
| MySQL | 8.0+（或兼容集群） |
| Redis | 7.x+（本地开发可选用） |
| JDK + Android SDK | 用于构建 Android 客户端 |

### 1. 克隆仓库

```bash
git clone https://github.com/Linux-System-0/chatbox.git
# 或 git clone https://codeberg.org/Linux-System-0/chatbox.git
cd chatbox
```

### 2. 配置

复制示例环境变量文件并按需修改：

```bash
cp deploy/.env.example deploy/.env
```

| 变量 | 说明 | 默认值 |
|---|---|---|
| `MYSQL_SHARDS` | MySQL 分片：以 `;` 分隔分片，每个分片内以 `\|` 分隔主库/从库 | `mysql://user:pass@127.0.0.1:3306/chatbox` |
| `REDIS_URL` | 单节点 Redis 连接串 | `redis://127.0.0.1:6379` |
| `REDIS_CLUSTER_NODES` | Redis 集群种子节点 URL（逗号分隔）；优先于 `REDIS_URL` | — |
| `REDIS_ENABLED` | 是否启用 Redis | `true` |
| `REDIS_URL` | Redis 连接串 | `redis://127.0.0.1:6379` |
| `REDIS_ENABLED` | 是否启用 Redis | `true` |
| `JWT_SECRET` | 用于签发认证令牌的密钥 | — |
| `HTTP_BIND` | HTTP 监听地址 | `0.0.0.0:8080` |
| `RUST_LOG` | 日志级别（`error`…`trace`） | `info` |

### 3. 启动后端

```bash
cd backend
cargo run
```

### 4. 启动 Web 客户端

```bash
cd web
npm install
npm run dev
```

### 5. 构建 Android 客户端

```bash
cd android
./gradlew assembleDebug
```

> 上述命令为开发者工作流。未设置 `MYSQL_SHARDS` 时后端默认使用内存存储，因此本地
> 开发无需 MySQL 或 Redis。运行完整技术栈请执行
> `cd deploy/docker && docker compose up --build`。

---

## ⚙️ 工程设计考量

以下非功能性需求驱动整体架构，并作为一等设计关注点跟踪。右列为**优化后**的方案，
它有意替换朴素做法（例如为每条已读回执都去锁一行），以在重并发下依然保持正确。

| 关注点 | 高并发下的失效模式 | 优化方案 |
|---|---|---|
| **投递语义** | 重试导致消息重复 | 至少一次 + `client_msg_id` 幂等键，去重表 / Redis `SETNX` 带 TTL，消费端幂等 |
| **顺序** | 并发写入使会话内消息交错 | 每会话单调序列号（`INCR` / Snowflake + 本地排序）；客户端检测空洞并重同步 |
| **实时扇出** | 超大群写放大 | 推拉混合：小群写时扇出，大群读取时扇出（游标模型） |
| **连接路由** | 广播到所有节点浪费扇出 | Redis 连接注册表 + 定向发布到持有连接的网关 |
| **热点实体** | 某个热门群/用户打满单个分片 | 分片计数器、本地缓存、Key 复制、批量/合并写入、按键工作队列 |
| **数据库并发** | 锁等待、死锁、热行 | 短事务、按固定顺序加锁、乐观版本校验、不持有长事务、热点路径避免 `SELECT ... FOR UPDATE` |
| **分片与事务** | 跨分片 2PC 拖垮延迟 | 按 `conversation_id` / `user_id` 分片；不变量约束在单分片内；跨分片用 Saga/outbox |
| **主从延迟** | 写后读读到旧数据 | 读己之写路由（短时粘主 / LSN Token），延迟监控，读到陈旧数据时回退主库 |
| **主从健康** | 从库静默停摆仍提供旧数据 | 心跳 + 延迟阈值；自动将延迟从库剔出读池 |
| **故障切换** | 脑裂双写 | 自动提升主库 + fencing token / 租约；写入方拒绝过期 epoch |
| **缓存击穿** | 热点 Key 失效瞬间大量请求打到库 | 单飞 / 请求合并（按 Key），逻辑过期 + 异步刷新，按 Key 互斥 |
| **缓存雪崩** | 大规模同时过期压垮数据库 | TTL 抖动、发布预热、多级缓存（进程内 + Redis）、缓存层熔断 |
| **缓存穿透** | 查不存在的 Key 绕过缓存 | 空值缓存、布隆过滤器、落库前校验 |
| **缓存不一致** | 数据库与缓存分歧 | Cache-Aside + 写时失效（删除）、延迟双删或基于 CDC 的失效、版本化 Key |
| **缓存热 Key** | 单个 Key 超出单分片能力 | 前置本地缓存、Key 复制、读从库、拆分热 Key |
| **超时** | 单个慢依赖拖垮连接池 | 截止时间传递、逐跳预算、每个 `await` 有界、按依赖设置超时 |
| **重试** | 重试风暴放大故障 | 重试预算、指数退避 + 全抖动、仅幂等重试、`Retry-After` 带抖动 |
| **级联失败** | 单个依赖拖垮全局 | 熔断器、舱壁（按依赖隔离连接池）、负载丢弃、优先级队列 |
| **限流** | 流量尖峰耗尽容量 | 本地令牌桶 + Redis Lua 全局配额；按块租借配额；按用户/设备/IP/会话 |
| **Rust 并发** | 阻塞异步运行时、锁竞争 | Tokio 多线程；绝不阻塞；每连接 Actor + 有界 `mpsc`；分片 Map；带超时的 `try_lock`；有界通道实现背压 |
| **内存 / 背压** | 慢消费者撑爆无界队列 | 每连接有界发送队列、丢弃最旧/断连策略、端到端背压 |
| **可观测性** | 故障时两眼一抹黑 | RED/USE 指标、分布式追踪、延迟直方图、饱和度告警、分阶段计时 |

### 优化的消息流水线

1. **接入** —— 客户端携带 `client_msg_id` 发送；API 校验鉴权并限流。
2. **持久化** —— 单个短事务写入消息 + outbox 行（同分片，无跨分片操作）。
3. **发布** —— Outbox 中继发布到持久化消息流；发送方立即收到携带会话 `seq` 的确认。
4. **扇出** —— Worker 解析接收者（缓存成员关系），应用推/拉策略，按连接注册表路由。
5. **投递** —— 持有连接的网关通过有界队列经 WebSocket 推送；离线设备重连后推进游标。
6. **确认** —— 每设备确认推进已读游标；回执合并批量处理，绝不每条回执更新一行。

```mermaid
sequenceDiagram
    participant C as 客户端
    participant API as API 节点
    participant DB as MySQL Outbox
    participant MQ as 持久化消息流
    participant F as 扇出 Worker
    participant GW as 网关节点
    participant R as Redis 注册表

    C->>API: send(client_msg_id)
    API->>DB: 短事务：消息 + outbox
    API-->>C: ack(seq, client_msg_id)
    DB->>MQ: outbox 中继发布
    MQ->>F: 投递事件
    F->>R: 解析在线连接
    R-->>F: 网关 + 设备 + 连接
    F->>GW: 推送到持有连接的网关
    GW-->>C: WebSocket 帧
    C->>API: ack(设备游标)
```

---

## 🗺️ 路线图

- [x] 项目脚手架（后端 workspace、Web、Android）
- [x] 认证：ID 登录、密码验证、会话、多设备
- [x] 单聊：文字 + 文件传输、离线队列
- [x] 联系人：按 ID 添加、好友申请、备注/昵称
- [x] 群聊：角色、权限、`@` 提及、禁言/踢人/拉人
- [x] 搜索：类型（通配符）、时间范围、内容；排序与跳转到消息
- [x] 单聊拉黑名单
- [x] Redis 集成（缓存、在线状态、连接注册、限流、扇出）
- [x] Redis 集群
- [x] MySQL 主从读取路由
- [x] MySQL 多分片分表
- [ ] MySQL 自动故障切换与副本健康剔除
- [x] 插件系统（客户端语言包/主题、服务端插件）
- [x] 可观测性（结构化 tracing）与限流
- [ ] 压力测试与容量调优

---

## 🤝 贡献指南

在初始脚手架就绪后，欢迎参与贡献。

1. Fork 仓库并创建功能分支：
   `git checkout -b feat/your-feature`。
2. 遵循**英文优先**约定：代码、注释与文档均使用英文。
3. 保持提交粒度小，并使用清晰的规范化提交信息。
4. 在可行的情况下补充测试。
5. 提交 Pull Request，说明变更内容与动机。

---

## 📄 许可证

本项目基于 **GNU 通用公共许可证第 3 版**（GPL-3.0-only）授权。
完整条款见 [`LICENSE`](LICENSE) 文件。

---

## 🙏 致谢

本项目基于 [SvelteKit](https://kit.svelte.dev/)、[Kotlin](https://kotlinlang.org/)、
[Rust](https://www.rust-lang.org/)、[MySQL](https://www.mysql.com/) 与
[Redis](https://redis.io/) 构建。