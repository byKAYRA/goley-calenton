# Goley Emulator & Client Runtime — Farklı Bilgisayarda Çalıştırma ve Taşınabilirlik Rehberi

Bu belge, projeyi sıfırdan başka bir Windows bilgisayarda klonlayıp çalıştıracak geliştiriciler ve kullanıcılar için hazırlanmıştır.

---

## 1. Sistem ve Yazılım Gereksinimleri

| Bileşen | Gereksinim | Not |
| :--- | :--- | :--- |
| **İşletim Sistemi** | Windows 10 veya Windows 11 (64-bit) | Win32 API ve DLL enjeksiyonu için |
| **Rust Toolchain** | Stable (1.80+) | `rustup` kurulu olmalı |
| **32-Bit Derleme Hedefi** | `i686-pc-windows-msvc` | **Client tarafı için zorunlu** |
| **C++ Build Tools** | Visual Studio 2022 C++ Build Tools | MSVC bağlayıcısı (linker) için |
| **Oyun Dosyaları** | Goley Türkçe İstemcisi (`BinaryTr.exe`) | Telifli oyun dosyaları repoda yer almaz |

---

## 2. Kolay Kullanım: Masaüstü Arayüzlü Başlatıcılar (GUI)

Kullanıcıların komut satırıyla uğraşmaması için her iki projeye de saf Win32 arayüzlü bağımsız `.exe` başlatıcılar eklenmiştir:

### 🎮 İstemci Başlatıcı (`goley-launcher.exe`):
* **Dosya Seçimi:** İlk açılışta veya "Gözat..." butonuyla `BinaryTr.exe` konumunu seçtirir.
* **Hafızada Tutma:** Seçilen yolu `goley_launcher_config.json` dosyasına kaydeder ve sonraki açılışlarda hatırlar.
* **Tek Tıkla Başlatma:** Gerekli tüm `NMRunEnv` ortam değişkenleri ve `goley_shim.dll` enjeksiyonunu otomatik yaparak oyunu açar.
* **Derleme:**
  ```powershell
  cd Gemini/goley-client
  cargo build --release -p goley-launcher-gui
  # Çıktı: target\i686-pc-windows-msvc\release\goley-launcher.exe
  ```

### 🖥️ Sunucu Başlatıcı (`server-launcher.exe`):
* **Başlat / Durdur:** "SUNUCUYU BAŞLAT" ve "SUNUCUYU DURDUR" butonlarıyla sunucuyu arka planda yönetir.
* **Durum Bildirimi:** Auth (8000), Entry (2270) ve Lobby (2271) dinleyicilerinin açık olduğunu arayüzde gösterir.
* **Derleme:**
  ```powershell
  cd Gemini/goley-server
  cargo build --release -p server-gui
  # Çıktı: target\release\server-launcher.exe
  ```

---

## 3. Komut Satırı ile Çalıştırma Adımları

### 1. Adım: Rust 32-bit Desteğini Ekleyin (Bir defaya mahsus)
```powershell
rustup target add i686-pc-windows-msvc
```

### 2. Adım: Sunucu Emülatörünü Başlatın (`goley-server`)
```powershell
cd Gemini/goley-server
.\start.ps1
```

### 3. Adım: İstemciyi Başlatın (`goley-client`)
```powershell
cd Gemini/goley-client
.\launch.ps1
# Veya farklı bir oyun dizini için:
.\launch.ps1 "D:\Oyunlar\Goley\BinaryTr\BinaryTr.exe"
```

---

## 4. Olası Hatalar ve Çözümleri

| Belirti / Hata | Olası Neden | Çözüm |
| :--- | :--- | :--- |
| `goley-boot.exe not found` | İlk çalıştırmada henüz derlenmemiş | `launch.ps1` otomatik derler; veya `cargo build --release --target i686-pc-windows-msvc` çalıştırın. |
| `BinaryTr.exe bulunamadı` | Oyun varsayılan `C:\Joygame\...` yolunda değil | Arayüzden "Gözat..." ile seçin veya `launch.ps1 "GERCEK_YOL\BinaryTr.exe"` yazın. |
| `GameGuard Error 99` | Shim enjeksiyonu gecikti veya devre dışı kaldı | `--late-inject-ms 3000` değerini sistem hızınıza göre 4000-5000 yapabilirsiniz. |
