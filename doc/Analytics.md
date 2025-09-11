# Analytics 模式（Scheme A）：持久 Python/R REPL（JSON-RPC）

本文档规划一种无需 LSP/Kernel 的最小可行实现：在 TUI 中通过持久子进程（Python 或 R）执行代码，使用简单 JSON-RPC（NDJSON）协议交互；支持代码预览与结果回显，可逐步扩展为“Notebook-like”体验。

## 目标

- 在 TUI 中启动持久 Python 或 R 会话，跨多次执行保持状态。
- 通过自然语言生成代码（可编辑），确认后发送到解释器执行。
- 回显 stdout/stderr、错误、变量摘要；图表先以文件路径展示。
- 与现有 `--repl` 与聊天流无缝协作；引入 `--python` 与 `--r` 模式开关。

非目标（本阶段不做）
- 富媒体双向协议（完整 Jupyter Kernel）与内嵌位图渲染。
- 高级变量浏览器、数据帧分页浏览、图交互。

## CLI 与模式

- 新增开关：
  - `--python`：启动 Python 会话
  - `--r`：启动 R 会话
- 互斥关系：`--python` 与 `--r` 与 `--shell` 互斥；可与 `--repl`/`--chat` 连用。
- 典型用法：
  - `sgpt --python --repl`
  - `sgpt --r --repl`
  - 无 `--repl` 时为一次性执行（读取输入→执行→退出会话）。

## 架构

### 1) 进程管理器（`src/process/`）
- 职责：启动/监控/重启解释器；建立 stdin/stdout 双向流；提供心跳与超时控制。
- 接口草案：
  - `enum InterpreterType { Python, R }`
  - `struct ProcessHandle { child: Child, stdin: ChildStdin, stdout: ChildStdout }`
  - `fn start(interpreter: InterpreterType) -> Result<ProcessHandle>`
  - `async fn send(&mut self, line: &str)` / `async fn recv_line(&mut self) -> Option<String>`

### 2) 执行引擎（`src/execution/`）
- 职责：封装协议与超时、解析响应、构造 `ExecutionResult`。
- `struct ExecutionResult { success: bool, output: String, errors: Vec<String>, variables: HashMap<String, String>, plots: Vec<String> }`
- 约束：限制单次输出大小（截断提示）、单次执行超时、可取消（丢弃后续响应）。

### 3) TUI 模式与事件
- 事件扩展（在现有 `src/tui/events.rs` 基础上增加）：
  - `ExecuteCode { language: InterpreterType, code: String }`
  - `CodeExecutionResult(ExecutionResult)`
  - `SwitchInterpreter(InterpreterType)`
  - `ShowVariables`
  - `ClearSession`
- 快捷键建议：
  - `Ctrl+P` 切换 Python，`Ctrl+R` 切换 R
  - `Enter` 发送代码，`Esc` 取消预览/弹窗
  - `Ctrl+V` 查看变量摘要，`Ctrl+L` 清理会话（重新启动解释器）

### 4) UI 布局（MVP→增强）
- MVP：双栏
  - 左：聊天/历史与执行结果回显（保留现有聊天体验）。
  - 右：代码预览（可编辑/确认执行）。
- 增强：三栏
  - 左：聊天/历史
  - 右上：代码预览（`syntect` 高亮）
  - 右下：结果与变量摘要（图表仅显示文件路径/打开提示）

## 协议（NDJSON）

- 传输：每请求/响应一行 JSON（换行分隔）。
- 请求：
```json
{"id":"uuid","method":"execute","params":{"code":"print('hi')","capture_output":true,"capture_plots":true}}
```
- 响应（成功）：
```json
{"id":"uuid","result":{"success":true,"output":"hi\n","variables":{"x":"int(1)"},"plots":["/tmp/plot_1.png"]}}
```
- 响应（失败）：
```json
{"id":"uuid","error":{"message":"NameError: x is not defined","traceback":"..."}}
```
- 其他：
  - 心跳：`{"method":"ping"}` / `{"result":"pong"}`（可选）
  - 执行结束标记：以 `id` 对应一次收敛；多段输出通过累积 `output`/`stderr` 字段或追加事件。

## 解释器侧注入

### Python
- 启动命令：`python -u - <<'PY'` 或 `python -u` 后注入 bootstrap。
- Bootstrap 要点：
  - 重定向 `sys.stdout/sys.stderr` 以捕获输出（或 `contextlib.redirect_stdout`）。
  - 执行代码：`exec(code, globals)`；捕获异常并格式化。
  - 变量摘要：遍历 `globals()`，生成 `name: type/shape`（如 `DataFrame(1000x5)`）。
  - 图表：设置 `matplotlib` 非交互后端（Agg），`plt.savefig(tmp_path)` 收集路径。

### R
- 启动：`R --slave --vanilla`（或 `Rscript --vanilla`）。
- 捕获输出：`capture.output({...})`；变量列表：`ls()`；图表：`png(file); plot(...); dev.off()`。
- MVP 可后置，先完成 Python。

## 安全与资源

- 超时：默认 30s（可配置）。
- 输出限制：如 64KB 截断（提示已截断）。
- 清理：临时图像文件周期清理；会话重启时清空。
- 风险提示：执行用户/模型生成的任意代码，默认“先预览后执行”；提供“只读目录”建议与白名单策略（后续）。

## 依赖与配置

- 依赖：
  - 现有：`serde_json`、`tokio/process`。
  - 可选（阶段二）：`syntect`（代码高亮）。
- 环境：本机需可用 `python`（或 `python3`），R 支持时需 `R`/`Rscript`。

## 实现步骤（分期）

1) 阶段一：Python MVP（2–4 天）
- CLI：新增 `--python`/`--r`（先实现 `--python`）。
- 进程：`process/python.rs` 启动/通信/重启；NDJSON 收发。
- 执行：`execution/python.rs` 协议封装、超时/截断、结果解析。
- TUI：双栏布局与事件；结果回显在聊天区；图表路径展示。

2) 阶段二：TUI 强化（2–3 天）
- 三栏布局、`syntect` 代码高亮、变量摘要面板、快捷键完善。

3) 阶段三：R 支持（3–5 天）
- `process/r.rs` 与 `execution/r.rs`；最小执行/输出/变量/图表保存。

4) 阶段四：增强特性（按需排期）
- ASCII 图表渲染/外部打开、会话保存/恢复、资源配额、错误弹窗与重试。

## 里程碑与验收

- MVP 验收：
  - `sgpt --python --repl` 可持续执行多段代码，变量状态保持，错误可读提示；
  - 输出可见且带截断提示；生成图表的路径可见；
 - 会话可清理/重启。

本方案在不引入 LSP/Kernels 的前提下，最小化依赖与复杂度，快速提供“像 Notebook 一样可执行”的 TUI 体验，并为后续对接 Jupyter Kernel 或富媒体渲染保留空间。

## 计划与进度跟踪（Python / R）

- 更新日期：2025-09-11

### Python 计划（MVP）

进度清单：
- [x] 方案与设计文档（本文件）
- [x] CLI 参数：新增 `--python`（已接入 REPL 路由）
- [ ] 进程管理器：`process/python.rs`（进行中：启动/收发已接入；心跳/重启待补）
- [ ] 执行引擎：`execution/python.rs`（进行中：骨架已建；暂由 TUI 直接收发）
- [ ] 协议 Bootstrap（进行中：stdout/stderr 捕获、变量摘要已实现；图表保存待加）
- [x] TUI 事件扩展：`CodeExecutionResult` 等（已接入最小路径）
- [x] 指令→代码（经 LLM 生成）→确认→执行 流程（首版，按 `e` 执行/`r` 重复）
- [ ] TUI 双栏布局：代码预览 + 结果回显（基础快捷键）
- [ ] 安全与资源：超时、输出上限、临时文件清理
- [ ] 验收用例与使用说明更新（README 片段）

里程碑（完成标准）：
- [ ] `sgpt --python --repl` 可持续执行多段代码（通过自然语言→生成代码→确认→执行），变量保持；错误可读；输出可见并有截断提示；会话可清理/重启

### R 计划（MVP）

进度清单：
- [ ] 方案细化（基于本文件，R 适配补充）
- [x] CLI 参数：新增 `--r`（仅参数，未接入执行流程）
- [ ] 进程管理器：`process/r.rs`（启动/心跳/重启/收发，调用 `R`/`Rscript`）
- [ ] 执行引擎：`execution/r.rs`（NDJSON 协议/超时/截断/解析）
- [ ] 协议 Bootstrap（R：`capture.output`、`ls()` 变量摘要、`png()/dev.off()` 图表保存）
- [ ] TUI 复用事件与布局（必要的语言切换/标识）
- [ ] 安全与资源：超时、输出上限、临时文件清理
- [ ] 验收用例与使用说明更新（README 片段）

里程碑（完成标准）：
- [ ] `sgpt --r --repl` 可持续执行多段代码，变量保持；错误可读；输出可见并有截断提示；会话可清理/重启

通用风险/阻塞：
- [ ] 本机解释器可用性与依赖（Python: matplotlib；R: grDevices/IRkernel 非必须）检测与降级策略
- [ ] 大输出/长时间执行的体验与取消机制
