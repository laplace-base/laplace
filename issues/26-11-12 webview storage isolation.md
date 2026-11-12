Этап 1.8 плана миграции (`26-10-01 migration plan.md`). Проверка и запись решения; код — только если проверка найдёт общее хранилище.

После 1.7 каждое окно, и лаунчер тоже, открыто на своей схеме `lapp-<name>://localhost` (на Windows и Android — `http://lapp-<name>.localhost`), то есть на своём origin. Профиль WebView у всех окон по-прежнему один: `data_directory` не задаётся. Хранилища, разделённые по origin — `localStorage`, `sessionStorage`, IndexedDB, Cache API, — в одном профиле уже раздельны. Подтвердить это нужно проверкой, а не выводом. Отдельные хранилища на окно (`data_directory`, `data_store_identifier`, профили androidx.webkit) остаются запасным вариантом. На Android первых двух нет вовсе, так что origin там — единственный механизм.

Что не делится по origin и требует проверки:

- **Cookies** привязаны к хосту, а не к схеме. На Linux и macOS у всех лаппов хост один — `localhost`, — поэтому cookies могут оказаться общими. Если WebKit их для своей схемы вообще не хранит, это тоже нужно записать. На Windows и Android хосты разные (`lapp-<name>.localhost`), но надо проверить, что cookie с `Domain=localhost` не ставится.
- **Работает ли хранилище на своей схеме вообще.** wry регистрирует схемы как безопасные. Нужно убедиться, что на WebKitGTK и WKWebView `localStorage` и IndexedDB не выключены и не обнуляются при перезапуске.

Проверка (лаунчер — окно с другим origin, второй лапп не нужен):

1. В консоли окна echo:
   ```js
   localStorage.setItem("probe", "echo");
   document.cookie = "probe=echo; path=/";
   indexedDB.open("probe", 1).onupgradeneeded = e => e.target.result.createObjectStore("s");
   ```
2. В консоли окна лаунчера проверить, что ничего не видно: `localStorage.getItem("probe")` — `null`, в `document.cookie` нет `probe`, `(await indexedDB.databases()).map(d => d.name)` без `probe`.
3. Перезапустить приложение и проверить в окне echo, что все три значения сохранились.
4. На Windows и Android дополнительно: `document.cookie = "probe2=echo; Domain=localhost; path=/"` из echo не видно в лаунчере.
5. Повторить на Linux и Android, по возможности на Windows и macOS.

Если что-то окажется общим — на Linux и Windows задавать окну лаппа `data_directory` (например `app_local_data_dir()/webview/<name>`), на macOS — `data_store_identifier`. Это отдельные процессы WebView на лапп; оформить отдельной задачей с оценкой памяти.

Попутно записать:

- Данные, сохранённые до 1.7 на `tauri://localhost`, на новых origin не видны. Переносить их не нужно: сами лаппы хранилищем не пользуются, а htmx держит в `localStorage` только кэш истории (`htmx-history-cache`).
- Удаление лаппа (7.4) должно удалять и данные его origin: WebKitGTK — `WebsiteDataManager`, Android — `WebStorage.deleteOrigin`, WebView2 — очистка по origin. Tauri даёт только `clear_all_browsing_data` на весь профиль, так что понадобится своя обвязка. Записать в план, пункт 7.4.

Готово, когда результаты проверки по платформам и итоговое решение записаны в `docs/architecture.md` (раздел «Окна» или новый «Хранилище WebView»), а пункт 7.4 плана дополнен удалением данных origin.
