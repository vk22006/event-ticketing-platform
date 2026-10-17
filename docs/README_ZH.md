# EventPro - 端到端活动生命周期与票务平台

[English](../README.md) | [தமிழ்](README_TA.md) | 简体中文 | [हिन्दी](README_HI.md) | [Bahasa Indonesia](README_ID.md)

EventPro 是一个全栈活动管理与票务平台，旨在管理活动的完整生命周期，从最初的规划和发布，到座位预订、门票生成以及预订后的管理。平台为三类主要用户提供相应的工作流程：活动参与者、活动组织者和平台管理员。

![首页](img/home_page.png)

本仓库仅使用由 **Rust 和 Axum** 编写的高性能、内存安全后端（`eventpro-rs/`），并已永久停用旧版 Express.js 实现。该架构通过严格的数据库事务边界、行级悲观并发锁以及有界的 multipart 请求内存限制来保障安全，同时使用 MiniJinja 服务端渲染模板。

---

## 核心功能与用户工作流程

### 活动参与者
- **活动目录与发现**：浏览已发布的活动，可实时按类别、城市和文本进行筛选，并使用服务端分页。
- **分级座位与预订**：选择普通入场票或自定义座位等级（例如 VIP、普通票或学生票）的票数，价格会立即重新计算。
- **事务式预订**：在原子数据库事务中预留门票，并通过行锁防止热门活动出现超额售票。
- **电子票与打印**：查看具有唯一字母数字编码的电子票。专用打印页面会移除导航等界面元素，方便实体打印或导出为 PDF。
- **自助取消**：可在活动开始前取消现有预订，系统会实时自动恢复活动和对应座位等级的可用容量。

### 活动组织者
- **活动生命周期管理**：管理活动的完整生命周期，包括草稿、已发布、已取消和已完成状态。
- **自定义座位等级**：为每场活动配置独立的座位等级、自定义名称、座位容量和票价。
- **横幅图片管理**：上传活动横幅，系统会严格验证文件内容、自动分配随机文件名，并在删除活动时自动清理相应文件。
- **销售与收入仪表板**：跟踪每场活动的门票总销量、剩余容量和产生的收入。

### 平台管理员
- **全平台管理**：查看全平台指标，包括注册用户总数、活跃组织者数量、已发布活动总数以及平台总收入。
- **活动审核**：查看所有组织者发布的活动，并拥有完整的管理、编辑和删除权限。
- **用户审计**：监控账户注册情况，并核验平台内的角色分配。

---

## 架构概览

| 组件 | 技术 / 实现方式 | 说明 |
| :--- | :--- | :--- |
| 运行时与语言 | Rust 1.80+ / Tokio 多线程异步运行时 | 内存安全、支持高并发的异步运行时 |
| Web 框架 | Axum 0.7（Tower 中间件生态） | 可组合服务、类型安全的提取器以及自定义中间件 |
| HTML 模板 | MiniJinja | 编译进二进制文件，并自动进行上下文相关的 HTML 转义 |
| 数据库层 | SQLx 0.8 | 异步 MySQL 连接池，提供严格的事务保证 |
| 会话存储 | `tower-sessions` | 基于 MySQL 的会话存储（`tower_sessions` 表） |
| 密码安全 | `bcrypt` | 使用标准 Blowfish cost 10 进行验证，并兼容历史密码哈希 |
| CSRF 防护架构 | 加密令牌 | 对会修改数据的 POST 请求执行恒定时间令牌验证 |
| 文件上传 | 有界流式 multipart 解析器 | 检查文件魔数；单个文件上限为 5 MB，整个请求上限为 8 MB |
| 静态文件服务 | `tower-http` ServeDir | 直接从 `public/` 提供静态文件，并设置 `X-Content-Type-Options: nosniff` |

---

## 配置参考

通过环境变量管理配置。将项目根目录或 `eventpro-rs/` 目录中的 `.env.example` 复制为 `.env`：

```bash
cp .env.example .env
```

| 变量 | 是否必需 | 默认值 | 用途 |
| :--- | :--- | :--- | :--- |
| `DATABASE_URL` | 必需（或使用单独的 `DB_*` 变量） | 无 | MySQL 连接字符串：`mysql://user:pass@host:port/database` |
| `SESSION_SECRET` | 必需 | 无 | 用于签名会话的密钥（至少 16 个字符） |
| `DB_HOST`、`DB_PORT`、`DB_USER`、`DB_PASSWORD`、`DB_NAME` | 备用配置 | 无 | 未设置 `DATABASE_URL` 时使用的独立连接参数 |
| `PORT` | 可选 | `3001` | Axum HTTP 服务器监听的端口 |
| `HOST` | 可选 | `127.0.0.1` | 绑定的网络接口 |
| `APP_ENV` | 可选 | `development` | 设置为 `production` 后，会强制为会话 Cookie 启用 `Secure=true` |
| `PUBLIC_DIR` | 可选 | `public` | 存放 CSS、客户端 JavaScript 和静态媒体文件的目录 |
| `UPLOAD_DIR` | 可选 | `PUBLIC_DIR/uploads` | 存放已上传横幅图片的文件系统路径 |

*安全说明：Rust 配置加载器实现了自定义调试格式化功能，会将 `DATABASE_URL` 和 `SESSION_SECRET` 隐藏为 `[REDACTED]`。密钥不会输出到标准输出或日志聚合系统。*

---

## 快速开始

### 1. 前置条件
- **MySQL Server 8.0+**，运行在端口 `3306`
- **Rust 工具链 1.80+**（Cargo 和 rustc）
- **Node.js 20+**（可选，仅在运行 `benchmarks/` 中的独立基准测试套件时需要）

### 2. 数据库设置
使用你选择的 MySQL 客户端初始化数据库结构和示例数据：

```bash
# 创建数据表并应用约束
mysql -u root -p < database/schema.sql

# 导入初始类别、示例活动、座位等级和演示账户
mysql -u root -p event_lifecycle < database/seed.sql
```

如果会话管理表 `tower_sessions` 尚不存在，Rust 后端会在启动时自动创建该表。核心应用表（`users`、`events`、`categories`、`seat_types`、`bookings`、`tickets`）均保持不变。

### 3. 运行 Rust 后端

在仓库根目录执行：

```bash
cd eventpro-rs
cargo run
```

服务器会编译程序、连接 MySQL、验证会话存储，然后监听 `http://localhost:3001`（或你配置的 `PORT`）。

运行状态探测接口：
- **存活探测**：`GET http://localhost:3001/health`（返回 JSON 状态）
- **就绪探测**：`GET http://localhost:3001/ready`（测试实际 MySQL 查询是否可以执行）

---

## 演示账户

种子数据库中包含代表各类用户的预配置账户：

| 角色 | 邮箱 | 密码 | 权限 |
| :--- | :--- | :--- | :--- |
| 管理员 | `admin@eventpro.com` | `Admin@1234` | 完整的平台仪表板、活动审核和全局分析权限 |
| 组织者 | `bob@events.com` | `Organizer@1` | 创建和编辑活动、上传横幅、配置票种等级 |
| 参与者 | `charlie@mail.com` | `User@1234` | 浏览活动目录、选择座位、预订门票、查看门票和取消预订 |

*较早开发种子数据中的旧密码 `admin123`、`org123` 和 `user123` 也受到 bcrypt 哈希器支持。*

## 安全与并发架构

### 1. 受 CSRF 保护的 POST 登出

会话销毁仅允许通过 `POST /auth/logout` 执行，并且必须提供有效的 CSRF 令牌。顶部导航栏和仪表板侧边栏都会呈现语义化表单，并包含隐藏的 `csrf_token` 字段。
- 使用有效令牌提交登出表单后，系统会销毁 MySQL 中的会话，并重定向到公开主页。
- 缺少 CSRF 令牌或使用伪造令牌的请求会被拒绝，并返回 HTTP 403 Forbidden；用户当前的已认证会话会继续保持有效。
- 访问 `GET /auth/logout` 的请求会收到 HTTP 405 Method Not Allowed，从而防止网络爬虫、链接预取器或浏览器历史导航意外终止会话。

### 2. 缓解会话固定攻击

每次用户成功登录或注册账户时，系统都会在填充用户详细信息之前立即调用 `session.cycle_id()`。这会生成新的会话密钥，从而消除会话固定漏洞。

### 3. 并发控制与事务完整性

当大量用户同时购买热门活动最后几张门票时，可能会产生严重的竞态条件：
- **悲观锁定**：预订处理程序会先启动事务，并对活动记录和目标座位等级记录执行 `SELECT ... FOR UPDATE` 锁定。
- **严格容量检查**：容量检查在锁定的事务内执行。如果请求的门票数量超过可用座位数，事务会干净地回滚，不会留下部分记录。
- **座位等级一致性**：如果活动定义了座位等级，没有选择有效等级的预订请求会在分配座位之前被拒绝。
- **原子化取消**：取消预订时，系统会在同一个事务边界内更新预订记录，并恢复活动和座位等级的容量。
- **冲突重试**：系统会检查唯一门票编码（`EVT-{event_id}-BKG-{booking_id}-{random_hex}`）是否违反唯一性约束，并在发生冲突时自动重试。

### 4. 有界 Multipart 上传与文件系统路径限制

活动横幅上传由独立的流式解析器处理，并执行严格的限制：
- **请求正文大小限制**：创建和编辑活动的接口受到 8 MB 的总请求大小限制（`MAX_EVENT_FORM_BODY_SIZE`），防止通过无限制请求发起拒绝服务攻击。
- **文件流大小限制**：横幅文件上限为 5 MB（`MAX_BANNER_SIZE`）。超出限制时，流式处理会停止并返回明确错误。
- **文本字段大小限制**：非文件表单字段限制为 64 KB（`MAX_TEXT_FIELD_SIZE`），防止超大文本负载耗尽内存。
- **文件魔数字节检查**：系统会检查 JPEG、PNG、WebP 和 GIF 的标准魔数签名。用户提供的文件扩展名和 MIME 标头都不被视为可信信息。
- **安全文件名**：上传文件会被赋予由密码学安全随机值生成的十六进制文件名（`event-{timestamp}-{hex}.{ext}`）。路径穿越序列（`..`、`/`、`\`）无法改变目标目录。
- **生命周期清理**：如果上传后数据库插入失败，系统会立即删除遗留图片文件。删除活动时会移除动态横幅，同时安全保留默认种子资源（`event-*.jpg`）。
- **防止内容嗅探**：上传文件通过 `X-Content-Type-Options: nosniff` 标头提供，防止浏览器执行伪装成其他类型的 HTML 或脚本内容。

### 5. 模拟支付流程

结账页面（`GET /bookings/payment`）和确认接口（`POST /bookings/confirm`）用于模拟电商结账流程。系统不会调用 Stripe 或 PayPal 等外部支付处理商。预订确认后，平台会立即生成有效的门票编码，以供演示使用。

## 部署注意事项

### 生产环境部署说明
1. **反向代理设置**：在生产环境中，将 EventPro 部署在 Nginx、Caddy 或云负载均衡器后方，由其终止 TLS 并将流量转发到 `127.0.0.1:3001`。
2. **Cookie 安全性**：设置 `APP_ENV=production`，会话层便会自动为 Cookie 设置 `Secure=true` 和 `SameSite=Lax`。
3. **持久化上传卷**：在 Docker 或 Kubernetes 等容器化环境中，将 `UPLOAD_DIR` 映射到持久化共享卷，使上传的媒体文件在 Pod 重启后仍然保留。
4. **水平扩展**：基于数据库的会话表 `tower_sessions` 支持在轮询负载均衡器后运行多个后端实例，而无需启用会话粘性。

## 目录结构

```text
.
├── benchmarks/               <- 独立的性能基准测试与验证套件
│   ├── package.json          <- 基准测试工具依赖（autocannon、mysql2）
│   ├── results/              <- 保留的基准测试测量结果与原始数据
│   └── *.js                  <- 基准测试工具和正确性测试
├── database/                 <- 数据库 SQL 结构、种子数据和迁移脚本
│   ├── schema.sql            <- 应用数据表和关系约束
│   ├── seed.sql              <- 类别、演示活动和用户
│   └── migrations/           <- 新增索引和优化
├── public/                   <- 客户端资源与媒体文件存储
│   ├── css/                  <- 应用样式
│   ├── js/                   <- 客户端 JavaScript 工具
│   └── uploads/              <- 上传的活动横幅
├── eventpro-rs/              <- Rust / Axum 后端服务
│   ├── Cargo.toml            <- Crate 配置与依赖项
│   ├── src/
│   │   ├── main.rs           <- 服务器入口、信号处理和监听器绑定
│   │   ├── lib.rs            <- 根库模块定义
│   │   ├── config.rs         <- 环境变量加载与凭据脱敏
│   │   ├── csrf.rs           <- CSRF 令牌创建与恒定时间验证
│   │   ├── error.rs          <- 集中式错误处理与状态码映射
│   │   ├── middleware.rs     <- 角色认证与会话守卫
│   │   ├── upload.rs         <- 有界 multipart 解析器与魔数字节检查
│   │   ├── models/           <- 数据库模型（User、Event、Booking、SeatType、Ticket、Category）
│   │   └── routes/           <- 路由处理程序（events、bookings、auth、tickets、admin、health）
│   ├── templates/            <- MiniJinja 服务端渲染 HTML 模板
│   └── tests/                <- 全面的集成测试与正确性测试套件
└── README.md                 <- 整合后的项目文档
```

## 测试与验证流程

Rust 代码库提供了全面的测试覆盖，包括单元测试、模板渲染测试和数据库集成测试。

在 `eventpro-rs/` 目录中运行验证套件：

```bash
cd eventpro-rs

# 1. 格式检查
cargo fmt --check

# 2. 严格 lint 检查
cargo clippy --all-targets -- -D warnings

# 3. 标准测试运行
cargo test

# 4. 严格的真实 MySQL 数据库集成测试（97 个测试）
# Linux / macOS：
STRICT_DB_TESTS=1 cargo test -j 2

# Windows PowerShell：
& { $env:STRICT_DB_TESTS='1'; cargo test -j 2 }
```

### 严格数据库测试保证

指定 `STRICT_DB_TESTS=1` 时：
- 测试会连接到端口 `3306` 上运行的真实 MySQL 实例。
- 如果无法连接数据库或数据库配置不正确，测试套件会立即 panic，而不会静默跳过集成测试。
- 测试会实际执行真实事务、并发预订模拟、编码冲突重试以及横幅文件存储生命周期检查。

## 许可证

本项目采用 MIT 许可证。详情请参阅 [LICENSE](LICENSE)。
