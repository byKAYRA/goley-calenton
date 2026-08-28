#  Goley Calenton (İstemci Başlatıcı & Shim Enjeksiyon Sistemi)

> [!NOTE]
> **Bağımsız Proje Bildirimi:** Bu proje, **tamamen bağımsız (standalone)** bir istemci platformudur (`byKAYRA/goley-calenton`). Kendi ayrı deposunda barındırılır ve derlenmesi ya da çalıştırılması için sunucu kaynak kodlarına ihtiyaç duymaz. Herhangi bir uyumlu Goley sunucusuyla (veya yerel [`goley-salgo`](https://github.com/byKAYRA/goley-salgo) ile) birlikte kullanılabilir.

Bu proje, **Goley (TR)** istemcisinin (`BinaryTr.bin` / `BinaryTr.exe`) modern 64-bit/32-bit Windows sistemlerinde bağımsız olarak başlatılabilmesi, koruma mekanizmalarının aşılması ve yerel/özel sunucu emülatörüne bağlanabilmesi için geliştirilmiş tersine mühendislik ve istemci başlatma platformudur.

---

##  Projenin Amacı

1. **Koruma Katmanlarını Aşma:** Orijinal istemci içinde bulunan Themida paket açma döngülerini ve eski GameGuard doğrulamalarını (Status 380 gate, Error99 yoklamaları) bellek düzeyinde yamalayarak oyunun çökmeden Login ekranına ulaşmasını sağlamak.
2. **Erken Bellek Enjeksiyonu (Early Injection):** Askıya alınmış süreç (Suspended Process) oluşturup oyunun PE Giriş Noktasına (OEP) ulaşmadan önce `goley_shim.dll` kütüphanesini enjekte etmek.
3. **Kullanıcı Dostu Masaüstü Başlatıcı:** Kullanıcının yalnızca oyunun `BinaryTr.bin` dosyasını seçip tek tıkla oyunu başlatmasını sağlamak (arka planda otomatik `.exe` kopyalama ve ortam değişkenlerini hazırlama).

---

##  Dizin ve Dosya Yapısı

```
goley-calenton/
├── .cargo/
│   └── config.toml               # Derleme ayarları (target-dir = "APP")
├── APP/
│   └── CALENTON/
│       └── release/              # Derlenmiş Dağıtım Dosyaları (32-bit i686)
│           ├── goley-launcher.exe  <-- [Özel İkonlu Masaüstü GUI Başlatıcı]
│           ├── goley-boot.exe      <-- [Suspended Launcher & Injector CLI]
│           ├── goley_shim.dll      <-- [Bypass, Hook & Patch DLL]
│           └── patches.toml        <-- [Dinamik Bellek Yama Yapılandırması]
├── crates/
│   ├── goley-launcher-gui/       # Win32 GUI Başlatıcı kaynak kodları
│   │   ├── src/main.rs           # GUI döngüsü, .bin->.exe kopyalama, başlatma
│   │   ├── build.rs              # İkon gömme derleme betiği
│   │   └── app.ico               # Masaüstü uygulama ikonu
│   ├── goley-boot/               # Süreç başlatıcı ve enjektör (CLI)
│   └── goley-shim/               # PE kancalama (Hooking), Themida & GameGuard bypass
├── docs/                         # Tersine mühendislik kanıtları, dökümler, RVA analizleri
├── bigpickle.md                  # Kapsamlı Tersine Mühendislik & Durum Raporu
├── build.bat                     # Projeyi tek tıkla derleyen betik
├── launch.ps1                    # CLI / PowerShell üzerinden hızlı başlatma betiği
├── Cargo.toml                    # Rust Workspace manifest dosyası
├── LICENSE                       # MIT Lisansı
└── README.md                     # Bu dosya
```

---

##  Dokümantasyon ve Araştırma Raporları

* **`docs/`**: İstemci başlatma zinciri (`boot.md`), bellek kancalama (Hooking) kayıtları, Themida açma adımları ve RVA analiz kanıtlarını içerir.
* **`bigpickle.md`**: Goley tersine mühendislik sürecinin tüm aşamalarını, bellek taramalarını (RPM), kart veri şablonlarını (`session+0x3500` RB-tree) ve teknik bulguları detaylandıran kapsamlı durum raporudur.

---

##  Modüller ve Görevleri (Crates)

### 1. `goley-launcher-gui` (Masaüstü Başlatıcı)
* Saf Windows Win32 API ile geliştirilmiş, harici bağımlılık gerektirmeyen hafif GUI başlatıcıdır.
* Oyunun kurulu olduğu dizindeki `BinaryTr.bin` dosyasını seçtirir ve bu yolu `goley_launcher_config.json` dosyasında hatırlar.
* `.bin` dosyasını aynı klasörde `BinaryTr.exe` olarak hazır hale getirir.
* Gerekli ortam değişkenlerini (`NMRunEnv_VER`, `NMRunEnv_DATA_1`) otomatik oluşturup `goley-boot.exe`'yi doğru argümanlarla tetikler.

### 2. `goley-boot` (Enjektör CLI)
* Hedef oyun sürecini `CREATE_SUSPENDED` bayrağı ile başlatır.
* Süreç belleğini gözlemler, Themida'nın bellek açma (unpacking) aşamasını tamamlamasını bekler.
* `goley_shim.dll` dosyasını `LoadLibraryW` veya bellek manipülasyonu ile sürece güvenli şekilde enjekte eder ve ana iş parçacığını (`ResumeThread`) devam ettirir.

### 3. `goley-shim` (Bypass & Hook Kütüphanesi)
* 32-bit dinamik bağlantı kütüphanesidir (`i686-pc-windows-msvc`).
* **GameGuard 380 Gate:** `0x009374DB` adresindeki ilk doğrulamayı yamalar.
* **Periodic GameGuard Status 99:** `0x0093BB67` adresindeki periyodik yoklamayı başarı (status: 0) dönecek şekilde düzenler.
* **Dinamik Bellek Yamaları:** Harici `patches.toml` dosyasındaki RVA ve bayt dizilimlerini dinamik olarak çalışma zamanında uygular.

---

##  Derleme ve Çalıştırma

### Gereksinimler
* [Rust Toolchain](https://www.rust-lang.org/)
* 32-bit Windows Target'ı:
  ```powershell
  rustup target add i686-pc-windows-msvc
  ```
* Visual Studio C++ Build Tools

### Derleme
Proje kök dizinindeki **`build.bat`** dosyasını çalıştırmanız yeterlidir:
```cmd
build.bat
```
Derlenen tüm dosyalar otomatik olarak **`APP\CALENTON\release\`** klasörüne kopyalanacaktır.

### Çalıştırma
1. `APP\CALENTON\release\goley-launcher.exe` dosyasını açın.
2. **Gözat...** butonuna basarak oyunun `BinaryTr.bin` veya `BinaryTr.exe` dosyasını seçin.
3. **GOLEY'İ BAŞLAT** butonuna basın.

> [!WARNING]
> Eğer sunucu ([`goley-salgo`](https://github.com/byKAYRA/goley-salgo)) arka planda açık değilse, oyun istemcisi Login ekranında sunucu yanıtı alamayacağı için kapanabilir veya çökebilir. Oyuna girmeden önce sunucuyu başlatmanız önerilir.

---

##  Teşekkürler (Special Thanks)

Bu projenin gelişimine katkıda bulunan ve destek veren değerli topluluk üyelerine teşekkür ederiz:

* [**@uintptr**](https://github.com/0x1-1) — Verdiğiniz ilham ve topluluğa sunduğunuz işler için...
* [**@Özkan Çırak**](https://github.com/ozkancirak) — Proje altyapısı, genel plan ve kesilen iletişim için...
* [**@WlayerX**](https://github.com/WlayerX/goley-server-tools) — Eski projeleri arşivlediğiniz ve dağıttınız için, Teşekkürler.
* Ayrıca bu misyonla bitirilmiş bir proje zaten var. [**Revival Projesi**](https://playrevival.co)'ni inceleyin.
