@echo off
echo =======================================================
echo Goley Client Projesi Derleniyor (APP\CALENTON\release)...
echo =======================================================
cargo build --release --target i686-pc-windows-msvc

if %ERRORLEVEL% NEQ 0 (
    echo.
    echo [HATA] Derleme sirasinda bir sorun olustu!
    pause
    exit /b %ERRORLEVEL%
)

if not exist "APP\CALENTON\release" mkdir "APP\CALENTON\release"
copy /Y "APP\i686-pc-windows-msvc\release\goley-launcher.exe" "APP\CALENTON\release\" >nul
copy /Y "APP\i686-pc-windows-msvc\release\goley-boot.exe" "APP\CALENTON\release\" >nul
copy /Y "APP\i686-pc-windows-msvc\release\goley_shim.dll" "APP\CALENTON\release\" >nul
copy /Y "crates\goley-shim\patches\patches.toml" "APP\CALENTON\release\patches.toml" >nul

echo.
echo [BASARILI] Derleme tamamlandi!
echo Cikti Klasoru: APP\CALENTON\release\
echo.
pause
