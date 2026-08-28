# Goley RE kurulum durumu

Son denetim: 2026-08-15 22:41

## Oyun yolu

- Kanonik istemci: `C:\Joygame\Goley`
- Kullanıcı ortam değişkenleri `GOLEY_CLIENT_DIR` ve `GOLEY_DIR` bu yola ayarlandı.
- `%USERPROFILE%\Games\Goley` altında ikinci bir kopya bulunuyor; ana dosya hashleri kanonik kopyayla aynı.

## Kurulu çekirdek bileşenler

- x32dbg/x64dbg, ScyllaHide, TitanHide eklentisi, Scylla, GhostDbg, x64dbg-mcp
- Ghidra 12.1.2 + Temurin JDK 21 + yerelde derlenmiş GhidraMCP
- IDA Pro 9.4 + idalib MCP
- ImHex 1.38.1, Wireshark/TShark 4.6.8 portable, API Monitor v2r13
- Frida 17.17.0, Qiling 1.4.6, Unicorn 2.1.4, boofuzz 0.4.2, mitmproxy 12.2.3
- Rust/Cargo, i686 Windows hedefi, .NET SDK 10.0.400, CMake, Ninja, Maven, mdBook
- Magicmida ve unlicense paketleri ile rapordaki referans depoların ana bölümü

## MCP durumu

- `idalib`: etkin ve doğrulandı; 66 araç.
- `frida`: kayıtlı; initialize/list_tools testi geçti, 13 araç.
- `ghidra`: uçtan uca canlı; `GhidraMCPPlugin` 8080 portunda çalışıyor, stdio bridge üzerinden 27 araç ile açık `Goley.exe` programından adres/import okuma testi geçti.
- `wiremcp`: kayıtlı; initialize/list_tools testi geçti, 7 araç. Canlı yakalama için Npcap gerekli.
- `x64dbg`: x32dbg üzerinde `127.0.0.1:3000/mcp` canlı; initialize ve 80 araçlık `tools/list` testi geçti.

## Kalan kullanıcı/yönetici adımları

1. Npcap çalışıyor; TShark yerel adaptörleri ve loopback adaptörünü görüyor.
2. Visual Studio Build Tools/MSVC/Windows SDK eksiksiz; gerçek x86 C derleme testi geçti.
3. Rust `i686-pc-windows-msvc` derleme ve bağlama testi geçti.
4. GhidraMCP manifest uyumluluğu düzeltildi ve CodeBrowser yapılandırmasında etkinleştirildi. `Goley.exe` CodeBrowser içinde açık; `8080` sunucusu canlı.
5. Yeni MCP araç envanterinin yüklenmesi için Codex'i yeniden başlat veya yeni görev aç.
6. TitanHide kernel sürücüsü yalnızca ayrılmış Windows VM'de, gerekli boot/test-signing ayarları ve yeniden başlatma sonrasında yüklenir.

## Koşullu/opsiyonel

- AFL++, tcpflow ve binwalk v3 hazır kurulumda değil.
- `deku`, `binrw`, `unicorn-engine` gibi Rust crate'leri emulator Cargo workspace'i oluşturulduğunda `Cargo.toml` dosyasına eklenecek.
