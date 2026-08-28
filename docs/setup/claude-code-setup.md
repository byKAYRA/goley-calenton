# Claude Code CLI — Goley RE ortamı kurulumu

Codex'ten Claude Code'a geçiş. MCP'ler .codex/config.toml'dan taşındı; kurulum yolları
docs/environment/KURULUM_DURUMU.md ile aynı.

## 1. MCP sunucuları — .mcp.json (repo kökünde, hazır)

4 sunucu proje scope'unda: ghidra, frida, wiremcp, x64dbg.
Claude Code'u ilk kez repo kökünde başlatınca "use this project's MCP servers?" sorar → onayla.

    cd <ProjectRoot>
    claude
    /mcp

Beklenen: ghidra (~27), frida (~13), wiremcp (~7), x64dbg (~80).

CLI ile tek tek eklemek (alternatif):
    claude mcp add ghidra --scope project -- "<ToolsPath>\GoleyRE\MCP\GhidraMCP-current\.venv\Scripts\python.exe" "<ToolsPath>\GoleyRE\MCP\GhidraMCP-current\bridge_mcp_ghidra.py" --ghidra-server http://127.0.0.1:8080/
    claude mcp add frida  --scope project -- "<ToolsPath>\GoleyRE\MCP\frida-mcp-main\.venv\Scripts\frida-mcp.exe"
    claude mcp add x64dbg --scope project --transport http http://127.0.0.1:3000/mcp
    claude mcp add wiremcp --scope project -- "C:\Program Files\nodejs\node.exe" "<ToolsPath>\GoleyRE\MCP\WireMCP-main\index.js"

Yönetim: claude mcp list | claude mcp get <ad> | claude mcp remove <ad>

Transport farkı: x64dbg canlı HTTP (127.0.0.1:3000/mcp) — x32dbg açık + plugin yüklü olmalı.
ghidra/frida/wiremcp stdio (Claude Code başlatır). ghidra ayrıca Ghidra 8080 sunucusu +
açık program ister.

## 2. IDA Pro MCP — ayrı, plugin

    claude plugin marketplace add mrexodia/claude-marketplace
    claude plugin install ida-pro-mcp@mrexodia
    claude plugin update ida-pro-mcp@mrexodia

Ön koşul (sende var): IDA 9.4 + idalib global etkin + uv. Headless idalib tercih edilir.

## 3. Proje hafızası — CLAUDE.md ve AGENTS.md

Claude Code AGENTS.md'yi otomatik OKUMAZ, CLAUDE.md okur. Köprü kurduk: kökteki CLAUDE.md
içinde @AGENTS.md import var. Tek doğruluk kaynağı AGENTS.md — kuralları oraya yaz.
Doğrula: claude → /memory → @AGENTS.md çözülüyor mu.
Codex AGENTS.md, Claude Code CLAUDE.md okuduğu ve ikisi aynı AGENTS.md'yi kaynak aldığı için
ikisini aynı repoda dönüşümlü kullanabilirsin.

## 4. İzinler — RE araçları hassas

Oturumda: /permissions ile araç bazında. MCP araçları mcp__<sunucu>__<araç> kalıbında.
.claude/settings.json (repo kökü) ile baştan izinle:

    {
      "permissions": {
        "allow": ["mcp__ghidra__*","mcp__x64dbg__*","mcp__frida__*","mcp__wiremcp__*",
                  "Bash(cargo *)","Bash(git *)"]
      }
    }

mcp__x64dbg__* wildcard'ı memory write/patch dahil hepsini onaylar — bilerek aç.

Otonom uzun görev (Codex "durmadan ilerle" karşılığı):
- claude --permission-mode acceptEdits   → düzenlemeleri sormaz, komutları sorar
- claude --dangerously-skip-permissions  → hiç sormaz = Codex approval_policy="never" +
  danger-full-access karşılığı. SADECE bu izole RE makinesinde.
- Headless: claude -p "<görev>" --dangerously-skip-permissions

## 5. Hızlı geçiş kontrol listesi

1. cd goley-reverse && claude → proje MCP onayı
2. /mcp → 4 sunucu bağlı mı, araç sayıları
3. claude plugin install ida-pro-mcp@mrexodia → 5. sunucu
4. /memory → @AGENTS.md çözülüyor mu
5. x32dbg açık + plugin yüklü mü
6. Ghidra açık + program yüklü mü (8080)
7. .claude/settings.json izinleri
8. Büyük-adım prompt: claude -p "<prompt>" --dangerously-skip-permissions

## Codex ↔ Claude Code eşleştirme

| Codex | Claude Code |
|---|---|
| .codex/config.toml [mcp_servers.*] | .mcp.json mcpServers (+ IDA plugin) |
| AGENTS.md (otomatik) | CLAUDE.md → @AGENTS.md import |
| approval_policy = "never" | --dangerously-skip-permissions |
| sandbox_mode = danger-full-access | (varsayılan; sandbox yok) |
| model_reasoning_effort = ultra | /model, plan mode |
| plugin marketplace (mrexodia) | claude plugin marketplace add (aynı repo) |
| codex -p headless | claude -p headless |

---

## 6. Araçları başlatma — KONUMLAR (senin makinen)

Claude Code MCP'leri bu iki araca BAĞLANIR; onları sen elle açacaksın.

### x32dbg (client 32-bit olduğu için x32 — "x64dbg.exe" diye arama, o yok)
- Başlatıcı (x32/x64 seçtirir): `<ToolsPath>\GoleyRE\x64dbg\release\x96dbg.exe`
- Doğrudan 32-bit: `<ToolsPath>\GoleyRE\x64dbg\release\x32\x32dbg.exe`  ← BUNU aç
- MCP plugin YÜKLÜ (`x32dbg_mcp.dp32`), config: port 3000, `auto_start_mcp_on_plugin_load: true`
  → x32dbg açılır açılmaz `127.0.0.1:3000/mcp` kendiliğinden ayağa kalkar, .mcp.json oraya bağlanır.
- NOT: config'de `allow_memory_write` ve `allow_register_write` şu an FALSE. Predicate analizi
  (breakpoint + register/memory OKUMA + disasm + stack) bununla çalışır. Patch UYGULAMA aşamasında
  bu ikisini true yap: `...\x32\plugins\x32dbg-mcp\config.json`.

### Ghidra
- Başlatıcı: `<ToolsPath>\GoleyRE\ghidra_12.1.2_PUBLIC\ghidraRun.bat`
- Aç → projeyi/programı yükle (unpacked dump veya BinaryTr) → CodeBrowser'da GhidraMCP plugin
  8080 sunucusunu açar → .mcp.json'daki ghidra girdisi oraya bağlanır.

### Başlatma sırası (önemli)
1. x32dbg'yi aç → 3000 portu otomatik açılır
2. Ghidra'yı aç + program yükle → 8080 açılır
3. SONRA repo kökünde `claude` başlat → /mcp'de dördü de bağlı görünür
(frida ve wiremcp'yi Claude Code kendi başlatır, önceden açman gerekmez.)
