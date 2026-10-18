# Bloody Mouse Tray Light

[English](README.md) | **Русский**

Нативная версия для Windows на Rust. Показывает заряд батареи, частоту опроса и DPI мыши Bloody SG5 Pro в системном трее. Работает без запущенной Bloody Central; настройки и история полной зарядки хранятся рядом с программой.

## Запуск

Скачайте [`Bloody Mouse Tray Light.exe`](release/Bloody%20Mouse%20Tray%20Light.exe) и оставьте рядом с ним файл [`mouse_tray.json`](release/mouse_tray.json). Приложение рассчитано на 64-разрядную Windows.

В комплекте уже заданы русскоязычный интерфейс, тёмная тема и интервал опроса 5 секунд. Настройки можно изменить в окне программы.

## Сборка из исходников

Нужны Rust с MSVC toolchain и Microsoft Visual Studio C++ Build Tools. Из папки `bloody-mouse-tray-light` выполните:

```powershell
cargo build --release
```

Готовый файл появится в `target/release/bloody-mouse-tray-light.exe`.

## Основные возможности

- заряд Bloody SG5 Pro, цветной индикатор и режим значка батареи;
- частота опроса и DPI в подсказке значка;
- русский, украинский и английский языки;
- журнал отладки и запуск вместе с Windows по выбору пользователя.
