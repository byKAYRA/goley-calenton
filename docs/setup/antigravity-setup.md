# Antigravity IDE — Goley RE ortamı (Codex/Claude Code'dan taşındı)

Aynı MCP sunucuları, aynı AGENTS.md, aynı x32dbg/Ghidra. Sadece config formatı farklı.

## 1. MCP sunucuları — .agents/mcp_config.json (repo kökünde, hazır)

Proje bazlı, dört sunucu: ghidra, frida, wiremcp, x64dbg. Format Claude Code'un .mcp.json'una
neredeyse aynı; TEK fark: HTTP sunucusu (x64dbg) "url" değil "serverUrl" alanı kullanır.

- Proje bazlı yol: .agents/mcp_config.json  (bu repoda hazır)
- Global istersen: ~/.gemini/config/mcp_config.json (tüm Antigravity araçları paylaşır)

Antigravity'de görmek/yönetmek:
- IDE: Agent panel → "MCP Servers"
- CLI: /mcp
- 2.0: Settings → Customizations → Add MCP

Başlatma sırası (Claude Code ile aynı):
1. x32dbg aç → 127.0.0.1:3000 otomatik açılır (x64dbg serverUrl oraya bağlanır)
2. Ghidra aç + program yükle → 8080 açılır (ghidra MCP oraya bağlanır)
3. Sonra Antigravity'yi bu repoda aç → /mcp ile dördünü gör
   (frida/wiremcp'yi Antigravity kendi başlatır)

Notlar:
- serverUrl zorunlu; eski "url"/"httpUrl" desteklenmez.
- Bazı Antigravity sürümlerinde MCP "env" desteği kısıtlı. wiremcp'nin PATH env'i çalışmazsa,
  tshark'ı (Wireshark-4.6.8) sistem PATH'ine ekle ya da WireMCP'yi o dizinden çalıştır.
- Inline yorum ve top-level "timeout" desteklenmez (config saf JSON).

## 2. AGENTS.md — rules üzerinden bağlı

Antigravity AGENTS.md'yi OTOMATİK okumaz (CLAUDE.md'yi de). Kendi rules mekanizmasını kullanır:
- Global rules: ~/.gemini/GEMINI.md
- Workspace rules: .agents/rules/  (bu repoda .agents/rules/goley.md hazır)

.agents/rules/goley.md içinde `@../../AGENTS.md` referansı var → Antigravity AGENTS.md'yi dahil eder.
Rules dosyasını Antigravity'de "Always On" olarak işaretle (Rules panelinden), böylece her görevde
otomatik yüklenir.

Ekstra güvence (opsiyonel, global): ~/.gemini/GEMINI.md dosyana şu satırları ekle —
```
- Check for AGENTS.md in the project workspace and follow it.
- There may be additional AGENTS.md in sub-folders with scope-specific instructions.
```
Böylece hangi projeyi açarsan aç Antigravity AGENTS.md'yi arar.

## 3. Codex ↔ Claude Code ↔ Antigravity eşleştirme

| Konu | Codex | Claude Code | Antigravity |
|---|---|---|---|
| MCP config | .codex/config.toml [mcp_servers] | .mcp.json | .agents/mcp_config.json |
| HTTP MCP alanı | url | url/type:http | serverUrl |
| Proje hafızası | AGENTS.md (otomatik) | CLAUDE.md → @AGENTS.md | .agents/rules/*.md → @AGENTS.md |
| Global kural | — | ~/.claude | ~/.gemini/GEMINI.md |
| Kaynak | AGENTS.md (tek doğruluk) | AGENTS.md (tek doğruluk) | AGENTS.md (tek doğruluk) |

Üçü de aynı AGENTS.md'yi kaynak aldığı için üç araçta da dönüşümlü çalışabilirsin; kuralları hep
AGENTS.md'ye yaz.

## 4. Değişmeyen

- x32dbg, Ghidra, ScyllaHide/TitanHide, x64dbg-mcp plugin — hepsi <ToolsPath>\GoleyRE
  altında, IDE'den bağımsız. Konumlar docs/setup/claude-code-setup.md §6'da.
- x64dbg MCP config (memory/register write açık): x64dbg\release\x32\plugins\x32dbg-mcp\config.json
- UAC: goley-boot elevated çalışır; client'ı çalıştırırken UAC onayını insan vermeli.
