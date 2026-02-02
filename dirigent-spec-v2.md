# Dirigent - AI Coding Agent Orchestration IDE

## 專案概述

**Dirigent** 是一個專為 AI coding CLI 工具（Claude Code、Codex CLI、Gemini CLI）設計的輕量級 IDE。核心理念是讓開發者像「指揮家」一樣管理多個 AI agent session，而非傳統的「寫 code」工作流。

### 設計哲學

1. **極致效能** - 使用 GPUI (Zed 的框架) 實現 120fps，支援 9+ 同時 session
2. **模組化** - 最小核心 + 按需載入模組，避免 VS Code 式臃腫
3. **CLI 無關** - 核心不假設特定 CLI，透過 adapter 支援多種工具
4. **解決真實痛點** - 每個功能都對應社群反饋的具體問題
5. **純檔案系統** - 無資料庫，所有狀態存為 JSON/Markdown，像 Zed 一樣輕量
6. **尊重用戶資源** - 不偷用用戶的 API token，所有消耗透明可控

### 目標用戶

- 同時運行 3-9 個 AI coding session 的開發者
- 使用 Claude Code 為主，偶爾搭配其他 CLI
- 需要管理多專案同時開發
- 主力開發環境為 Windows（GPUI 已支援 Windows beta）

---

## 核心工作流

Dirigent 的核心不只是「多 terminal 管理」，而是 **Task-Driven Agent Orchestration**。

### 工作流總覽

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Task Queue                                   │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐                    │
│  │ Task A  │ │ Task B  │ │ Task C  │ │ Task D  │ ...                │
│  │ Priority│ │ Priority│ │ Priority│ │ Priority│                    │
│  └────┬────┘ └────┬────┘ └────┬────┘ └─────────┘                    │
│       │           │           │                                      │
│       ▼           ▼           ▼                                      │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │                    Auto Scheduler                            │    │
│  │            (當 Agent 空閒時，自動 pick 下一個 Task)            │    │
│  └─────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────┘
                              │
          ┌───────────────────┼───────────────────┐
          ▼                   ▼                   ▼
   ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
   │  Session 1  │     │  Session 2  │     │  Session 3  │
   │  Task A     │     │  Task B     │     │  Task C     │
   │  ⏳ Working │     │  ⚠️ Waiting │     │  ✅ Done    │
   │  ctx: 65%   │     │  ctx: 42%   │     │  ctx: 78%   │
   └──────┬──────┘     └──────┬──────┘     └──────┬──────┘
          │                   │                   │
          ▼                   ▼                   ▼
   ┌─────────────────────────────────────────────────────┐
   │                   Verification Gate                  │
   │         (自動執行測試，失敗則回傳錯誤讓 Agent 修復)    │
   └─────────────────────────────────────────────────────┘
                              │
                              ▼
   ┌─────────────────────────────────────────────────────┐
   │                    Human Review                      │
   │              (Change Summary + Diff Viewer)          │
   └─────────────────────────────────────────────────────┘
                              │
                              ▼
   ┌─────────────────────────────────────────────────────┐
   │                   Session Notes                      │
   │            (歸檔記錄，給人類看，不吃 Context)          │
   └─────────────────────────────────────────────────────┘
```

### Task 生命週期

```
┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
│  QUEUED  │───▶│ ASSIGNED │───▶│ WORKING  │───▶│ VERIFYING│───▶│  REVIEW  │
└──────────┘    └──────────┘    └──────────┘    └──────────┘    └──────────┘
                                      ▲               │               │
                                      │    ❌ Fail    │    ❌ Reject  │
                                      └───────────────┘               │
                                      ▲                               │
                                      │         🔄 Request Changes    │
                                      └───────────────────────────────┘
                                                                      │
                                                               ✅ Approve
                                                                      │
                                                                      ▼
                                                              ┌──────────┐
                                                              │   DONE   │
                                                              └──────────┘
```

---

## 技術架構

### UI 框架：GPUI

| 項目 | 說明 |
|------|------|
| 框架 | GPUI (Zed 的 GPU 加速 UI 框架) |
| 目標幀率 | 120fps |
| 渲染後端 | Windows: DirectX 11, macOS: Metal, Linux: Vulkan |
| 終端模擬 | alacritty_terminal crate |
| 狀態 | Windows beta (2025/09), 預計 1.0 於 Spring 2026 |

**選擇理由：**

- 效能優勢：9 個同時 session 時仍能保持 60fps+
- Zed 已驗證：終端實作可參考 `zed/crates/terminal`
- Windows 支援：已有 beta，開發期間會更穩定

### 儲存架構（純檔案系統，無資料庫）

```
專案根目錄/
│
├── .dirigent/                        # Dirigent 工作目錄
│   │
│   ├── config.json                   # 專案級設定
│   │
│   ├── tasks/                        # Task 資料 (JSON)
│   │   ├── task-001.json
│   │   ├── task-002.json
│   │   └── ...
│   │
│   ├── queue.json                    # Queue 排序狀態
│   │
│   ├── state.json                    # 運行時狀態
│   │
│   └── sessions/                     # Session Notes (Markdown)
│       ├── 2026-02-01/
│       │   ├── task-001-refactor-auth.md
│       │   └── task-002-add-tests.md
│       └── ...
│
├── CLAUDE.md                         # Claude Code 設定（非 Dirigent）
│
└── .gitignore

全域設定/
~/.config/dirigent/
├── settings.json                     # 全域設定（字體、快捷鍵等）
├── presets/                          # Quick Setup presets
└── plugins/                          # 已安裝模組
```

### 設計原則

| 原則 | 說明 |
|------|------|
| **檔案即資料** | 所有狀態都是 JSON/Markdown，無需資料庫 |
| **人類可讀** | 不用 Dirigent 也能直接看懂、手動編輯 |
| **可 Git 追蹤** | 團隊可以共享 tasks、config |
| **崩潰恢復** | 重啟後從檔案重建狀態 |
| **無背景程序** | 關掉就關掉，沒有 daemon |

### 模組系統

```
~/.config/dirigent/
├── plugins/                 # 已安裝模組
│   ├── input-detector/
│   ├── context-tracker/
│   └── ...
└── settings.json            # 全域設定
```

模組透過統一 API 與核心溝通，模組間可選擇性依賴增強功能。

---

## 模組規格

### Core Module（核心）

**大小：** ~8MB  
**功能：** 多終端管理 + 基本 UI  
**替代：** iTerm2 + tmux

| 功能 | 說明 |
|------|------|
| Multi-PTY Terminal | 同時運行多個 CLI instance |
| Grid Layout | 所有 session 可視（2x2, 1x4, 2x3, 3x3, 自訂）|
| Session Naming | 識別每個 session 用途 |
| Session Grouping | 同專案 session 可視覺分組（顏色標記）|
| Layout Profiles | 快速切換佈局 |
| File Tree | 瀏覽 + 拖曳路徑到終端 |
| Basic Settings | 字體、顏色、快捷鍵 |

**解決的痛點：**

- VS Code 太重（500MB+, 3-5s 啟動）
- iTerm tabs 隱藏其他 session
- tmux 學習曲線高
- 不知道哪個 session 對應哪個專案

**UI 佈局：**

```
┌─────────────────────────────────────────────────────────────────┐
│ [Layout ▼] [+New Session]                              [⚙️]     │
├─────────────────────────────────────────────────────────────────┤
│ ┌───────────────┬───────────────┬───────────────┐               │
│ │ Session 1     │ Session 2     │ Session 3     │               │
│ │ 🟢 my-app     │ 🟢 my-app     │ 🔵 client     │               │
│ │ [API重構]     │ [寫測試]      │ [Bug#42]      │               │
│ │               │               │               │               │
│ │ $ claude      │ $ claude      │ $ claude      │               │
│ │ > Working...  │ > Done ✓      │ > Waiting...  │               │
│ │               │               │               │               │
│ └───────────────┴───────────────┴───────────────┘               │
│ ┌───────────────┬───────────────┬───────────────┐               │
│ │ Session 4     │ Session 5     │ Session 6     │               │
│ │ 🔵 client     │ 🟡 marketing  │ 🟣 tools      │               │
│ │ [Feature]     │ [Landing]     │ [Script]      │               │
│ │               │               │               │               │
│ └───────────────┴───────────────┴───────────────┘               │
├─────────────────────────────────────────────────────────────────┤
│ [File Tree]  │  [Task Board]                                    │
└─────────────────────────────────────────────────────────────────┘
```

---

### Module: Input Detector

**大小：** ~500KB  
**依賴：** Core

| 功能 | 說明 |
|------|------|
| Pattern Detection | 偵測 `[Y/n]`, `?`, `>` 等輸入提示 |
| Session Status | idle / working / waiting / done |
| Visual Markers | Session card 上顯示 ⚠️ |
| Desktop Notifications | 離開時提醒 |
| Custom Patterns | 用戶自訂提示模式 |

**解決的痛點：**

- 跑多個 session 時錯過輸入提示
- 離開回來發現 agent 在等你

**狀態顯示：**

```
Session 狀態圖示：
⏳ Working    - Agent 正在執行
✅ Done       - 任務完成
⚠️ Waiting   - 等待輸入
💤 Idle      - 無活動
❌ Error     - 偵測到錯誤
```

---

### Module: Context Tracker

**大小：** ~1.5MB  
**依賴：** Core

**Subscription Mode（主要）：**

| 功能 | 說明 |
|------|------|
| Context % | 即時 context window 使用量 |
| Effective Context | 扣除 MCP overhead 後的實際可用 |
| Usage Limit | 4h / weekly 限額追蹤 |
| Threshold Warnings | 70% 黃色, 90% 紅色 |
| Compact Suggestions | 在邏輯斷點建議 compact |

**API Mode（次要）：**

| 功能 | 說明 |
|------|------|
| Cost Tracking | 累計花費 |
| Budget Alerts | 日/週/月預算 |
| Rate Monitoring | $/hour 追蹤 |

**解決的痛點：**

- Context 顯示不準確（顯示 0% 但實際 50%）
- 不知道 MCP 吃掉多少 context
- Auto-compact 太早觸發

**UI 顯示：**

```
Session Card 角落：
┌─────────────────┐
│ Session 1    72%│ ← Context 使用量
│ [API重構]       │
│ ⏳ Working      │
│                 │
│ MCP: -22%       │ ← MCP overhead（hover 顯示）
│ Effective: 50%  │
└─────────────────┘
```

---

### Module: Task Board

**大小：** ~3MB  
**依賴：** Core, Session Persistence (optional)

| 功能 | 說明 |
|------|------|
| Kanban View | TODO / IN PROGRESS / DONE |
| Plan Import | 從 markdown/spec 檔案匯入 |
| Task ↔ Session | 追蹤哪個任務在哪個 session |
| Progress Tracking | 透過 hooks 追蹤（optional）|
| Task Queue | 自動排程分配下一個任務 |
| Dependency Management | 任務依賴關係 |
| Step Details | 展開顯示任務內的步驟進度 |

**UI 位置：** 視窗底部可收合面板

```
┌─────────────────────────────────────────────────────────────────┐
│ Task Board                                    [Auto ✓] [+ Add]  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─ Queued (3) ──────────────────────────────────────────────┐  │
│  │                                                            │  │
│  │  1. 🔴 [Critical] Fix auth vulnerability                   │  │
│  │     Tags: security, backend                                │  │
│  │     Est: 30min | Waiting: 2min                             │  │
│  │     Ready ✓ (no dependencies)                              │  │
│  │     [Assign ▼] [Edit] [···]                                │  │
│  │                                                            │  │
│  │  2. 🟡 [Medium] Add user avatar upload                     │  │
│  │     Tags: frontend, s3                                     │  │
│  │     Est: 45min | Waiting: 15min                            │  │
│  │     ⏳ Blocked by: #3 (Setup S3 bucket)                    │  │
│  │                                                            │  │
│  │  3. 🟢 [Low] Setup S3 bucket                               │  │
│  │     Tags: infra, aws                                       │  │
│  │     Est: 15min | Waiting: 20min                            │  │
│  │     Ready ✓                                                │  │
│  │                                                            │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                 │
│  ┌─ In Progress (2) ─────────────────────────────────────────┐  │
│  │                                                            │  │
│  │  4. 🟡 [Medium] Refactor auth module → Session 1           │  │
│  │     ⏳ Working | 23min elapsed | ctx: 65%                  │  │
│  │                                                            │  │
│  │  5. 🟡 [Medium] Write API tests → Session 2                │  │
│  │     ⚠️ Waiting for input | 8min elapsed | ctx: 42%        │  │
│  │                                                            │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**解決的痛點：**

- 計畫和執行分離
- 不知道整體進度
- 手動追蹤任務繁瑣

---

### Module: Task Queue Scheduler

**大小：** 包含在 Task Board 中  
**依賴：** Task Board

**排程模式**

| 模式 | 說明 | 適用場景 |
|------|------|---------|
| **FIFO** | 先進先出 | 簡單專案 |
| **Priority** | 按優先級排序 | 有緊急任務 |
| **Dependency** | 考慮依賴關係 | 複雜專案 |
| **Smart** | 綜合以上 | 預設模式 |

**排程演算法（Smart 模式）**

```
當 Session 變成 idle 時觸發：

1. 篩選 status === 'queued' 且無 blocked 的 Tasks
2. 計算分數：
   score = (priority_weight × priority_score)
         + (age_weight × waiting_time_score)
         + (match_weight × tag_match_score)
3. 選擇最高分的 Task
4. 更新狀態，發送 prompt 到 Session
```

**Task 檔案格式**

```json
// .dirigent/tasks/task-001.json
{
  "id": "task-001",
  "title": "Refactor auth module",
  "description": "重構認證模組，改用 JWT",
  "priority": "high",
  "status": "queued",
  
  "dependencies": [],
  "tags": ["backend", "auth"],
  
  "assignedSession": null,
  "assignedAt": null,
  
  "estimatedMinutes": 45,
  "maxRetries": 3,
  "retryCount": 0,
  
  "verification": {
    "command": "npm test",
    "requiresHumanReview": true
  },
  
  "createdAt": "2026-02-01T10:00:00Z",
  "startedAt": null,
  "completedAt": null
}
```

---

### Module: Verification Gate

**大小：** ~2MB  
**依賴：** Core, Hooks Bridge

| 功能 | 說明 |
|------|------|
| Test Runner Integration | 偵測並執行專案的測試指令 |
| Auto-verify on Complete | Session 完成時自動觸發 |
| Result Parsing | 解析測試輸出，提取 pass/fail |
| Failure Routing | 失敗時自動將錯誤送回 Session |
| Retry Loop | 可配置最大重試次數 |
| Custom Scripts | 支援自定義驗證腳本 |

**Verification 流程**

```
Session 標記完成
       │
       ▼
┌─────────────────┐
│ 執行測試指令     │
└────────┬────────┘
         │
    ┌────┴────┐
    │         │
  Pass      Fail
    │         │
    ▼         ▼
┌────────┐ ┌─────────────────────┐
│ 進入   │ │ retryCount < max?   │
│ Review │ └──────────┬──────────┘
└────────┘            │
                ┌─────┴─────┐
                │           │
               Yes         No
                │           │
                ▼           ▼
          ┌──────────┐ ┌──────────┐
          │ 送錯誤訊息│ │ 標記為   │
          │ 回 Session│ │ Blocked  │
          │ retry++  │ │ 通知人類  │
          └──────────┘ └──────────┘
```

**Auto-detect 邏輯**

```
package.json 存在？ → "npm test"
Cargo.toml 存在？ → "cargo test"
pyproject.toml 存在？ → "pytest"
Makefile 有 test？ → "make test"
都沒有 → 提示用戶設定
```

**錯誤訊息回傳格式**

```markdown
## ❌ Verification Failed

**Test Results:** 21 passed, 2 failed

### Failures:

**1. auth.test.ts > should reject expired tokens**
```
Expected: 401
Received: 200
```

---

請修復上述問題後再次完成任務。

*Retry: 2/3*
```

**UI 顯示**

```
┌─────────────────────────────────┐
│ Session 1 - Verification        │
│                                 │
│ Unit Tests:        ✅ 23/23     │
│ Integration Tests: ❌ 6/8       │
│                                 │
│ Failures:                       │
│ • should handle edge case       │
│ • should validate input         │
│                                 │
│ Retry: 2/3                      │
│                                 │
│ [View Details] [Skip] [Manual]  │
└─────────────────────────────────┘
```

**解決的痛點：**

- Agent 說「完成了」但 test 沒過
- 手動跑測試、複製錯誤訊息很繁瑣
- 不知道什麼時候該人類介入

---

### Module: Session Persistence

**大小：** ~1MB  
**依賴：** Core

| 功能 | 說明 |
|------|------|
| Auto-save | 定期保存狀態到 `.dirigent/state.json` |
| State Content | 目標、修改檔案、進度、問題 |
| Resume | 載入先前 session 狀態 |
| Handoff | 高 context → 新 session 的 context 轉移 |
| Manual Checkpoint | 用戶觸發保存 |

**狀態檔案結構**

```json
// .dirigent/state.json
{
  "sessions": {
    "session-1": {
      "id": "session-1",
      "name": "API 重構",
      "status": "working",
      "currentTask": "task-001",
      "worktreePath": "./worktrees/session-1",
      "contextUsage": 65,
      "startedAt": "2026-02-01T10:30:00Z"
    }
  },
  "updatedAt": "2026-02-01T11:20:00Z"
}
```

**解決的痛點：**

- 關閉終端失去 context
- 切換專案要重新解釋 10-15 分鐘

---

### Module: Session Notes

**大小：** ~1MB  
**依賴：** Core, Hooks Bridge

**重要：Session Notes 是給人類看的，不是給 Claude 的 context。**

| 功能 | 說明 |
|------|------|
| Auto-generate | Task 完成時自動生成 Markdown 筆記 |
| File Change Tracking | 記錄修改的檔案（從 git/hooks）|
| Verification Results | 記錄測試結果 |
| Summary Mode | 可選擇是否讓 Claude 生成摘要 |
| Learnings Extraction | 提取 patterns，建議更新 CLAUDE.md |

**Token 消耗控制**

| 模式 | 說明 | Token 消耗 |
|------|------|-----------|
| **auto** | Claude 順便輸出摘要 | ~100-200/task |
| **manual** | 只記錄結構化資料，需要時手動觸發 | 0 |
| **none** | 不生成 Notes | 0 |

**預設為 `manual`，尊重 API 用戶的 token 成本。**

**設定**

```json
// .dirigent/config.json
{
  "sessionNotes": {
    "enabled": true,
    "summaryMode": "manual",
    "structuredDataOnly": true
  }
}
```

**筆記檔案格式**

```markdown
# Session Notes: Refactor Auth Module

**Task ID:** task-001  
**Session:** Session 1  
**Duration:** 45 minutes  
**Status:** ✅ Completed

---

## Files Changed

| File | Action | Changes |
|------|--------|---------|
| `src/auth/jwt.ts` | Created | +120 |
| `src/auth/middleware.ts` | Modified | +45, -30 |

**Total:** 4 files, +255 lines, -55 lines

## Verification Results

```
✅ Unit Tests:        23/23 passed
✅ Integration Tests: 8/8 passed
```

## Summary

_（若 summaryMode: auto）_
Objective: 重構認證模組，改用 JWT
Approach: 保留舊 session.ts，漸進式遷移
Outcome: 所有測試通過

_（若 summaryMode: manual）_
_未生成。點擊 [生成摘要] 補充。_

## Learnings

_建議考慮加入 CLAUDE.md：_
- 用 jose 而非 jsonwebtoken（ESM 相容性）

---

*Generated by Dirigent at 2026-02-01 11:15*
```

**儲存位置**

```
.dirigent/sessions/
├── 2026-02-01/
│   ├── task-001-refactor-auth.md
│   └── task-002-add-tests.md
└── 2026-02-02/
    └── ...
```

**Learnings 處理流程**

```
Session 完成，生成筆記
         │
         ▼
提取 Learnings（本地解析，零 token）
         │
         ▼
┌─────────────────────────────────┐
│ Dirigent 提示：                  │
│ 「發現可能有用的 patterns，      │
│  要加入 CLAUDE.md 嗎？」         │
│                                 │
│ [檢視] [忽略] [稍後]            │
└─────────────────────────────────┘
         │
         ▼
用戶手動決定是否更新 CLAUDE.md
（Dirigent 不會自動修改）
```

---

### Module: Change Summary

**大小：** ~1MB  
**依賴：** Core

| 功能 | 說明 |
|------|------|
| File Change Detection | 偵測 session 修改的檔案 |
| Change Categorization | 分類：新增/修改/刪除 |
| Impact Assessment | 影響範圍（UI/API/DB/Config）|
| Risk Level | 簡單規則判斷風險等級 |
| One-click Diff | 開啟 diff viewer |
| Summary Export | 複製成文字，貼到 PR description |

**風險判斷規則**

```
🔴 High Risk:
- 修改了 auth/security 相關檔案
- 修改了 database schema/migration
- 修改了 config/env 檔案
- 刪除了檔案

🟡 Medium Risk:
- 新增 API endpoint
- 修改核心業務邏輯

🟢 Low Risk:
- 測試檔案
- 文件更新
- 純 UI 變更
```

**UI 顯示**

```
┌─────────────────────────────────────────┐
│ Change Summary - Session 1              │
├─────────────────────────────────────────┤
│ 📊 12 files changed (+342 / -89)        │
│                                         │
│ 🔴 High Risk (需仔細 review)             │
│    src/auth/middleware.ts  [Diff]       │
│    db/migrations/002.sql   [Diff]       │
│                                         │
│ 🟡 Medium Risk                          │
│    src/api/users.ts        [Diff]       │
│                                         │
│ 🟢 Low Risk                             │
│    tests/* (6 files)       [Diff All]   │
│                                         │
│ [Open All Diffs] [Export Summary] [✓]   │
└─────────────────────────────────────────┘
```

**解決的痛點：**

- AI 產出太快，來不及 review
- 不知道該重點看哪些檔案
- PR review 成為瓶頸

---

### Module: Hooks Bridge

**大小：** ~1MB  
**依賴：** Core

| 功能 | 說明 |
|------|------|
| Hook Injection | 自動注入 Dirigent hooks 到 CLI config |
| Event Listener | 接收 CLI hook callbacks |
| Status Updates | 更新 session 狀態 |
| File Change Detection | 偵測檔案變更 |
| Progress Sync | 同步到 Task Board |

**支援的 Hooks（Claude Code）：**

- PreToolUse
- PostToolUse
- Stop

**解決的痛點：**

- 不知道 session 在做什麼
- 手動追蹤進度
- 複雜的 hook 設定

---

### Module: Skill Manager

**大小：** ~2MB  
**依賴：** Core

| 功能 | 說明 |
|------|------|
| Skill Browser | 視覺化瀏覽所有 skills/commands |
| Token Budget | 顯示 system prompt 空間使用 |
| Truncation Warning | Skills 超過 15k 限制時警告 |
| Enable/Disable | 切換 skills（利用 hot-reload）|
| Mode Presets | dev/review/test skill 組合 |
| Quick Insert | 點擊 → 插入到 session |
| Search | 模糊搜尋名稱/描述 |

**解決的痛點：**

- Skills 被靜默截斷
- 不知道載入了哪些 skills
- 太多 skills 記不住

---

### Module: Git Worktree

**大小：** ~1MB  
**依賴：** Core

| 功能 | 說明 |
|------|------|
| Worktree List | 顯示所有 worktree + branch |
| Quick Create | 一鍵從 branch 創建 |
| Session Binding | 連結 session 到 worktree |
| Auto-cleanup | 清理已 merge 的 worktree |

**解決的痛點：**

- 多 agent 修改同一 branch 造成衝突
- 不知道哪個 agent 在哪個 branch
- 手動管理 worktree 繁瑣

---

### Module: Ralph Loop

**大小：** ~1MB  
**依賴：** Core, Task Board (optional)

| 功能 | 說明 |
|------|------|
| Loop Status | 顯示 iteration N/max |
| Progress Log | 每個 iteration 做了什麼 |
| Completion Tracking | 監控完成承諾 |
| Overnight Mode | 批次排程，早上 review |
| Error Detection | 偵測卡住/無限迴圈 |
| Auto-pause | 異常時自動暫停 |

**Ralph Loop 原理**

```
while not done:
    run_prompt("完成任務，跑 test，修 error，重複直到沒錯誤")
    if context_full:
        compact_and_continue()
```

**UI 顯示**

```
┌─────────────────────────┐
│ Session 1 - Ralph Loop  │
│ Iteration: 7/20         │
│ ████████░░░░░░ 35%      │
│                         │
│ Last iteration:         │
│ - Fixed 3 test failures │
│ - 2 errors remaining    │
│                         │
│ [Pause] [Stop] [View Log]│
└─────────────────────────┘
```

**解決的痛點：**

- Overnight 跑完不知道做了什麼
- 無法視覺化 loop 進度
- 卡住時沒有警告

---

### Module: Broadcast

**大小：** ~500KB  
**依賴：** Core

| 功能 | 說明 |
|------|------|
| Multi-send | 發送 prompt 到選定的 sessions |
| Selective Send | 選擇目標 sessions |
| Template Variables | $SESSION_NAME, $WORKTREE, $PROJECT |
| Send History | 追蹤發送過什麼 |

**使用場景**

```
所有 session 都需要知道：
"API endpoint 改名了，從 /users 改成 /v2/users"

Broadcast:
[x] Session 1 (my-app)
[x] Session 2 (my-app)
[ ] Session 3 (client)  ← 不相關，不選

Message: "注意：API endpoint 已改為 /v2/users"

[Send]
```

**解決的痛點：**

- 手動複製貼上到每個 session
- 重複發送相同的澄清說明

---

### Module: Quick Setup

**大小：** ~500KB  
**依賴：** Core

| 功能 | 說明 |
|------|------|
| Project Type Detection | 自動偵測專案類型 |
| Preset Library | 內建常用 preset |
| Custom Presets | 自己保存 preset |
| One-click Apply | 自動複製 CLAUDE.md, settings.json 等 |
| Project Memory | 記住每個專案用什麼 preset |

**解決的痛點：**

- 每個新專案都要重新設定
- 不知道該用什麼配置
- 團隊成員配置不一致

---

### Module: Web Bridge

**大小：** ~1MB  
**依賴：** Core

| 功能 | 說明 |
|------|------|
| Drop to Session | 拖曳檔案到 session → 複製到 working directory |
| Quick Export | 快捷鍵複製 session output 到 clipboard |
| Path Memory | 記住常用的目標路徑 |
| Format Templates | 輸出格式可自訂 |

**解決的痛點：**

- 手動下載、找資料夾、拖曳
- 手動複製 CC output 到 Web

---

### Module: Multi-CLI

**大小：** ~2MB  
**依賴：** Core, Context Tracker (optional)

| 功能 | 說明 |
|------|------|
| CLI Adapters | 支援 CC, Codex CLI, Gemini CLI |
| Unified Status | 跨 CLI 一致的狀態顯示 |
| Cross-CLI Workflow | CC 規劃, Codex 實作 |
| Best-tool Routing | 根據任務類型選擇 CLI |

**Adapter 介面**

```rust
trait CLIAdapter {
    fn detect_status(&self, output: &str) -> SessionStatus;
    fn parse_context_usage(&self, output: &str) -> Option<f32>;
    fn get_input_patterns(&self) -> Vec<Regex>;
    fn format_command(&self, cmd: &str) -> String;
}
```

**解決的痛點：**

- 不同 CLI 要用不同工具
- 沒有 AI 間協作
- 供應商鎖定

---

## End-to-End Flow：完整任務流程

### 階段 ① CREATE：任務建立

**觸發方式**
- 用戶手動在 Task Board 建立
- 從 Markdown/Spec 檔案匯入
- Review 階段點擊 "Request Changes"

**涉及模組：** Task Board

**動作**
1. 用戶輸入 Task 資訊
2. 生成 `task-xxx.json` 檔案
3. 設定 `status = 'queued'`
4. 更新 `queue.json`

---

### 階段 ② QUEUE：排隊等待

**觸發方式**
- Task 建立後自動進入
- Verification 失敗後重新排隊
- Review Reject 後重新排隊

**涉及模組：** Scheduler

**動作**
1. 檢查 dependencies 是否滿足
2. 計算優先級分數
3. 等待 Session 空閒

---

### 階段 ③ ASSIGN：分配給 Session

**觸發方式**
- Session 狀態變為 idle
- 用戶手動分配

**涉及模組：** Scheduler, Core, Git Worktree

**動作**
1. 從 queue 選擇最高優先的 Task
2. 更新 Task 狀態
3. 構建 prompt，發送到 Session

**Prompt 範本**
```markdown
## Task: {title}

**Description:**
{description}

**Requirements:**
- {requirements}

**Verification:**
完成後執行 `{verification_command}` 確認測試通過
```

---

### 階段 ④ EXECUTE：執行任務

**涉及模組：** Core, Input Detector, Context Tracker, Hooks Bridge

**並行監控**
```
Session 執行中
     │
     ├── Input Detector → 需要輸入時通知
     ├── Context Tracker → >70% 時警告
     └── Hooks Bridge → 記錄檔案變更
```

---

### 階段 ⑤ VERIFY：驗證結果

**涉及模組：** Verification Gate

**動作**
1. 執行測試指令
2. Pass → Review
3. Fail → 送錯誤回 Session（retry）或標記 Blocked

---

### 階段 ⑥ REVIEW：人類審核

**涉及模組：** Change Summary, Task Board

**Review 介面**
```
┌─────────────────────────────────────────────────────────────────┐
│ Review: {Task Title}                                 [Session X]│
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│ Summary: {duration} | {files} changed                           │
│                                                                 │
│ Changes (by risk level)                                         │
│ 🔴 High: ...                                                    │
│ 🟡 Medium: ...                                                  │
│ 🟢 Low: ...                                                     │
│                                                                 │
│ Verification: ✅ All passed                                     │
│                                                                 │
│ Learnings: 💡 (optional)                                        │
│                                                                 │
│       [❌ Reject]    [🔄 Request Changes]    [✅ Approve]        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

### 階段 ⑦ COMPLETE：標記完成

**涉及模組：** Task Board, Core

**動作**
1. Task `status = 'done'`
2. Session `currentTask = null`, `status = 'idle'`
3. 觸發 Scheduler 分配下一個 Task

---

### 階段 ⑧ ARCHIVE：歸檔筆記

**涉及模組：** Session Notes

**動作**
1. 生成 Session Notes Markdown（零 token 或少量）
2. 存檔到 `.dirigent/sessions/`
3. 提取 Learnings，提示用戶

---

## 模組依賴圖

```
                              ┌─────────────────┐
                              │   Notifications │
                              └────────┬────────┘
                                       │
                    ┌──────────────────┼──────────────────┐
                    │                  │                  │
                    ▼                  ▼                  ▼
             ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
             │   Context   │    │    Core     │    │   Change    │
             │   Tracker   │    │             │    │   Summary   │
             └─────────────┘    └──────┬──────┘    └─────────────┘
                                       │
          ┌────────────────────────────┼────────────────────────────┐
          │                  │         │         │                  │
          ▼                  ▼         ▼         ▼                  ▼
   ┌─────────────┐    ┌─────────────┐     ┌─────────────┐    ┌─────────────┐
   │   Session   │    │    Input    │     │    Hooks    │    │   Skill     │
   │ Persistence │    │   Detector  │     │   Bridge    │    │   Manager   │
   └──────┬──────┘    └─────────────┘     └──────┬──────┘    └─────────────┘
          │                                      │
          │         ┌────────────────────────────┤
          │         │                            │
          ▼         ▼                            ▼
   ┌─────────────────────┐              ┌─────────────────┐
   │     Task Board      │              │  Verification   │
   │   (+ Scheduler)     │◄─────────────│     Gate        │
   └──────────┬──────────┘              └─────────────────┘
              │
    ┌─────────┼─────────┐
    │         │         │
    ▼         ▼         ▼
┌────────┐┌────────┐┌────────┐
│ Ralph  ││Session ││  Git   │
│ Loop   ││ Notes  ││Worktree│
└────────┘└────────┘└────────┘
```

---

## 安裝 Profiles

| Profile | 模組 | 大小 | 適合用戶 |
|---------|------|------|---------|
| **Minimal** | Core | ~8MB | DIY 用戶，只要多終端 |
| **Standard** | Core + Input + Context + Persistence + Task Board | ~15MB | 一般 CC 用戶 |
| **Power** | Standard + Verification + Worktree + Notes + Change Summary | ~20MB | 重度用戶 |
| **Full** | All modules | ~25MB | 想要所有功能 |

---

## 快捷鍵設計

### 全域

| 快捷鍵 | 功能 |
|--------|------|
| `Cmd+1-9` | 切換到 Session 1-9 |
| `Cmd+N` | 新增 Session |
| `Cmd+W` | 關閉當前 Session |
| `Cmd+K` | Quick Switch（搜尋 session/project）|
| `Cmd+\` | 切換 Layout |
| `Cmd+B` | 切換底部面板（Task Board）|
| `Cmd+Shift+B` | Broadcast 到選定 sessions |

### Session 內

| 快捷鍵 | 功能 |
|--------|------|
| `Cmd+Shift+E` | Export to clipboard |
| `Cmd+Shift+C` | 複製 session summary |
| `Cmd+Shift+D` | 開啟 Change Summary |

---

## 設定檔結構

### 專案設定

```json
// .dirigent/config.json
{
  "version": "1.0",
  
  "scheduler": {
    "mode": "smart",
    "autoAssign": true,
    "confirmBeforeAssign": false,
    "idleThresholdSeconds": 5
  },
  
  "verification": {
    "enabled": true,
    "autoDetect": true,
    "maxRetries": 3,
    "commands": {
      "unit": "npm test",
      "integration": "npm run test:integration"
    }
  },
  
  "sessionNotes": {
    "enabled": true,
    "summaryMode": "manual",
    "structuredDataOnly": true
  },
  
  "sessions": {
    "maxConcurrent": 5,
    "defaultCli": "claude"
  },
  
  "git": {
    "useWorktrees": true,
    "autoCommit": false
  }
}
```

### 全域設定

```json
// ~/.config/dirigent/settings.json
{
  "appearance": {
    "theme": "dark",
    "fontFamily": "JetBrains Mono",
    "fontSize": 14,
    "gridGap": 4
  },
  
  "notifications": {
    "desktop": true,
    "sound": false
  },
  
  "modules": {
    "inputDetector": {
      "customPatterns": ["\\[y/N\\]", "Press Enter"]
    },
    "contextTracker": {
      "warningThreshold": 0.7,
      "criticalThreshold": 0.9
    }
  }
}
```

---

## .gitignore 建議

```gitignore
# Dirigent - 不追蹤運行時狀態
.dirigent/state.json

# 可選：是否追蹤 session notes
# .dirigent/sessions/

# 應該追蹤
# .dirigent/config.json
# .dirigent/tasks/
# .dirigent/queue.json
```

---

## 開發路線圖

### Phase 1: Core + MVP（2-3 個月）

- [ ] Core 多終端實作（GPUI）
- [ ] Grid layout system
- [ ] Session naming + grouping
- [ ] Input Detector
- [ ] Basic file tree

### Phase 2: Task Management（1-2 個月）

- [ ] Task Board 基本版
- [ ] Task Queue + Scheduler
- [ ] Hooks Bridge
- [ ] Session Persistence

### Phase 3: Verification & Review（1-2 個月）

- [ ] Verification Gate
- [ ] Change Summary
- [ ] Session Notes
- [ ] Context Tracker

### Phase 4: Advanced（1-2 個月）

- [ ] Git Worktree
- [ ] Skill Manager
- [ ] Ralph Loop
- [ ] Broadcast

### Phase 5: Extended（Optional）

- [ ] Web Bridge
- [ ] Multi-CLI adapters
- [ ] Quick Setup

---

## 技術參考

- **alacritty_terminal**: Terminal emulation library
- **GPUI**: Zed's GPU-accelerated UI framework
- **Zed terminal**: `zed/crates/terminal` (reference implementation)
- **portable-pty**: Cross-platform PTY
- **git2-rs**: libgit2 Rust binding
- **CC 2.1.0**: Hot-reload skills, hooks, teleport
- **Boris workflow**: 5 local + 5-10 web Claudes, plan mode, verification loop
- **Ralph Loop**: Geoffrey Huntley's deterministic loop technique

---

## 附錄：社群痛點對照表

| 痛點 | 來源 | Dirigent 解決方案 |
|------|------|------------------|
| Context 顯示不準確 | GitHub issues | Context Tracker |
| Auto-compact 太早 | Reddit | Context Tracker + 手動控制 |
| MCP overhead 不透明 | Affaan | Context Tracker 顯示 effective context |
| Skills 被截斷 | Jesse (Superpowers) | Skill Manager |
| 切換專案失去 context | 社群普遍 | Session Persistence |
| 不知道哪個 session 在做什麼 | HN | Input Detector + Session Status |
| 多 agent 衝突 | dev.to | Git Worktree |
| 錯過輸入提示 | Reddit | Input Detector + Notification |
| Review 成為瓶頸 | Addy Osmani | Change Summary + Risk Assessment |
| 手動分配任務 | 社群普遍 | Task Board + Scheduler |
| Overnight 不知道進度 | Reddit | Ralph Loop + Timeline |
| 設定每個專案太繁瑣 | 社群普遍 | Quick Setup |
| Web ↔ CC 手動同步 | 用戶反饋 | Web Bridge |
| Agent 說完成但 test 沒過 | 社群普遍 | Verification Gate |
| 不知道 Agent 改了什麼 | 社群普遍 | Session Notes + Change Summary |

---

## 附錄：檔案格式參考

### Task 檔案

```json
// .dirigent/tasks/task-001.json
{
  "id": "task-001",
  "title": "Refactor auth module",
  "description": "重構認證模組，改用 JWT",
  "priority": "high",
  "status": "queued",
  "dependencies": [],
  "tags": ["backend", "auth"],
  "assignedSession": null,
  "assignedAt": null,
  "estimatedMinutes": 45,
  "maxRetries": 3,
  "retryCount": 0,
  "verification": {
    "command": "npm test",
    "requiresHumanReview": true
  },
  "createdAt": "2026-02-01T10:00:00Z",
  "startedAt": null,
  "completedAt": null
}
```

### Queue 狀態

```json
// .dirigent/queue.json
{
  "order": ["task-003", "task-004"],
  "blocked": {
    "task-005": ["task-003"]
  },
  "updatedAt": "2026-02-01T11:20:00Z"
}
```

### 運行時狀態

```json
// .dirigent/state.json
{
  "sessions": {
    "session-1": {
      "id": "session-1",
      "name": "API 重構",
      "status": "working",
      "currentTask": "task-001",
      "worktreePath": "./worktrees/session-1",
      "contextUsage": 65,
      "startedAt": "2026-02-01T10:30:00Z"
    }
  },
  "updatedAt": "2026-02-01T11:20:00Z"
}
```

### Session Notes

```markdown
# Session Notes: Refactor Auth Module

**Task ID:** task-001  
**Session:** Session 1  
**Duration:** 45 minutes  
**Status:** ✅ Completed

---

## Files Changed

| File | Action | Changes |
|------|--------|---------|
| `src/auth/jwt.ts` | Created | +120 |
| `src/auth/middleware.ts` | Modified | +45, -30 |

**Total:** 4 files, +255 lines, -55 lines

## Verification Results

✅ Unit Tests: 23/23 passed  
✅ Integration Tests: 8/8 passed

## Summary

_點擊 [生成摘要] 補充（需消耗少量 token）_

## Learnings

_建議考慮加入 CLAUDE.md：_
- 用 jose 而非 jsonwebtoken（ESM 相容性）

---

*Generated by Dirigent at 2026-02-01 11:15*
```
