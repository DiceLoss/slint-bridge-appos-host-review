# NX5 触摸队列修复部署与实测

2026-10-12，已将 `2de9168` 切换为 NX5 常驻程序，并通过用户实体触摸验证。用户确认暂停和恢复及时，也确认进行了额外连续点触。此前仅限内存画布/输入管道的队列回归见 [平台说明](software-platform.md#事件循环)；本轮没有新增滑动界面。

## 部署与回退文件

| 项目 | 本次记录 |
| --- | --- |
| 设备 | USB1 root、ADB `192.168.1.151:5555`，Linux 4.9.138 #233 |
| 启动 ID | `938c34d4-456a-4b1a-b532-678c0f92efd0`，未重启 |
| 常驻程序 | `/tmp/slint-bridge-touch-2de9168/display`，观测 PID 30267 |
| 源码 | `2de91683d6c3aa6c6abe1d03f739ba0545b362dd` |
| SHA256 | `425a1660192aa8886dea14d8d3d834a6a6a95a6035fe259fbdb4efab3d19af30` |
| 启动脚本 | `/tmp/slint-bridge-touch-2de9168/start-live.sh` |
| 回退程序 | `/tmp/slint-bridge-review-fixes/display`，源码 `bccf6b9` |
| 回退 SHA256 | `8aba5bfd344fc60f32f90ace2467206abc326956e20efdc5bcd1878d5b2fd107` |
| 回退启动脚本 | `/tmp/slint-bridge-review-fixes/start-live.sh` |

构建使用 `.cargo/nx5.toml`、`nx5-tde,slint/mcp` 和 Slint UI 调试信息。字体、SDK 目录、framebuffer 与触摸设备链接沿用原配置；没有修改 SDK、驱动或网络。通过 [部署工具](../test-app/tools/nx5/README.md#部署归档清理和截图) 上传到新路径，旧程序及启动脚本同时保存在设备和经 SHA256 校验的主机 `rollback/` 档案中。

切换前通过原生 MCP 暂停旧 PID 3972，确认帧计数稳定及 `fb-present` 已在队列上等待，再发送 SIGTERM。旧进程的三个 TDE MMZ 块由内核回收，VO 被关闭；这是信号结束旧程序的结果，不是新版正常析构测试。新程序启动前 MMZ 为 4900 KiB / 2 块。相关记录单独保存在 `old-exit.dmesg`、`old-exit.mmz`，确认旧所有者退出后才启动新程序。

## 实体触摸结果

只读观察器打开实际 `gsl1686` evdev，显式为该观察句柄选择 `CLOCK_REALTIME`，记录内核事件及 `SYN_REPORT` 时间戳；同时使用 Slint 原生 MCP 录制事件和查询状态。观察器不抓取设备独占权、不注入输入、不访问 framebuffer，也不加载厂商 SDK。

| 检查 | 结果 |
| --- | --- |
| 实际接触 | 10 组按下／抬起；用户确认包含额外连点 |
| Slint 事件 | 10 Pressed、5 Moved、10 Released、10 Exited，共 35 条，全部 Accepted |
| 丢失／未知事件 | 0 / 0 |
| 接触边沿匹配 | 20 条按下／抬起与 evdev 报告逐条匹配 |
| 最长分派时间差 | 约 15.3 ms；MCP 时间戳粒度为 1 ms，对应保守上界约 16.4 ms |
| 应用状态 | 5 次暂停／恢复；暂停时帧号稳定，恢复后继续增长 |
| 用户观察 | 暂停和恢复都及时 |

此时间差是内核完整触摸报告到 Slint 处理事件的时间，不包含传感器扫描和液晶扫描。小于 1 ms 的负时间差来自 MCP 毫秒时间戳截断，不作为负延迟解读。本轮没有在实际触摸期间额外注入任务积压，也未重跑 WBC、独立 ignored 硬件或 EIO 测试；不能把此前报告当作本版本的新测试。

实体录制结束后的一张截图记录到用户继续操作后的 PAUSED 状态，保存为 `post-touch-paused.png`；随后通过 MCP 恢复 RUNNING。最终属性查询帧号从 22966 增至 22982，[最终截图](../evidence/nx5-touch-deploy/2026-10-12-2de9168/final-running.png) 显示 Frame 22994 RUNNING。该次截图前可用内存为 47648 KiB，超过脚本要求的 16384 KiB。截图是原生软件快照，不替代显示扫描时延测量。

## 收尾与范围

11 个不用的普通文件先归档并校验摘要，再从设备删除，包括只读观察器、诊断记录和更早的 `c03c3bb` 程序副本；共用 SDK 与设备链接仍保留在该旧目录。`bccf6b9` 已成为明确的回退版本，[保留策略](../test-app/devices/nx5/deploy-retain.json) 同步更新。

最终 PID 30267 保持运行，RSS 10992 KiB、VmHWM 15800 KiB、3 个线程、MMZ 12100 KiB / 5 块，可用内存 53548 KiB。没有对应新版进程的新 OOM 或 MMZ 强制回收；持续 trace、WBC、输入观察器及事件录制均已关闭。当前环境快照见 [环境记录](current-environment.md)。

## 回退

重新核对设备身份和显示所有者，通过 MCP 暂停新版，并确认提交线程空闲。使用当时核对的 PID 停止新版，确认进程及显示所有权已释放后，在 USB1 root 终端运行保留的原启动脚本：

```sh
sh /tmp/slint-bridge-review-fixes/start-live.sh \
  > /tmp/slint-bridge-review-fixes/fixed-results/rollback-live.log 2>&1 < /dev/null &
```

不能同时启动两份显示程序，也不能直接沿用本文记录的 PID。回退后重新查询 MCP 句柄并确认帧计数；同时更新保留策略，使实际运行版与备用版一致。上述路径位于 tmpfs，设备重启后需要重新部署；本轮没有更改开机服务。

## 原始证据

档案位于本地 `evidence/nx5-touch-deploy/2026-10-12-2de9168/`，不随 Git 分发，需另行获取：

- `deployment-manifest.json`、`rollback/`：新版摘要、来源及切换前程序备份。
- `physical-touch.events`、`physical-touch-mcp.json`、`status-timeline.jsonl`、`user-observation.txt`：实体输入、Slint 接受结果、状态与用户反馈。
- `observe-touch.c`、`analyze-touch.py`、`physical-touch-summary.json`：一次性只读观察器、分析程序与匹配结果。
- `old-exit.*`、`final-dmesg.txt`、`after-cleanup.*`：旧进程退出和新版资源记录。
- `deploy/`、`cleanup/`：两轮归档清单与校验回执；`results.json` 为汇总。

取得档案后，可从仓库根运行 `python3 evidence/nx5-touch-deploy/2026-10-12-2de9168/analyze-touch.py` 重放匹配检查；它不会再次操作设备。
