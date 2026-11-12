Этап 1.5 плана миграции (`26-10-01 migration plan.md`). Зависит от 1.3 (`UpdateQuery`, решения пользователя) и 1.4 (реестр `LappsManager`).

Команды Tauri для лаунчера. Логика — в методах `LappsManager` с ошибками `LappError`, команды в `crates/app/src/api/laplace.rs` — тонкие обёртки, которые переводят ошибку в строку, как `handle_http`. Так всё проверяется тестами в `crates/app/tests/lapps.rs` без Tauri. Доступ только из окна лаунчера — задача 1.6, до неё команды доступны всем окнам.

- **`get_lapps`** — все лаппы реестра, отсортированные по имени. Для каждого: имя, манифест, решения пользователя, действующие права и `is_running` (для индикатора 6.6). Это новый сериализуемый тип в `laplace-settings` рядом с `LappUserSettings`, чтобы фронт этапа 6 взял его оттуда же. Захардкоженный список в `crates/front/src/lapps.rs` заменит этап 6, не эта задача.
- **`update_lapp(UpdateQuery) -> UpdateQuery`** — `LappUserSettings::update`: пишет только `lapps/config/<name>.toml` и возвращает применённую часть запроса.
  - `Lapp` должен знать путь к своему файлу решений: сейчас `Lapp::read` его вычисляет и не сохраняет.
  - Изменение и запись идут под write-блокировкой `user_settings` этого лаппа, поэтому два `update_lapp` подряд не теряют изменения друг друга.
  - Ошибка записи — новый вариант `LappError`.
- **Инстанс после `update_lapp`.**
  - Выключение или выдача/отзыв права останавливает работающий инстанс. На этапе 3 от прав зависит, какие импорты линкуются.
  - Новый инстанс поднимается как обычно: при старте платформы, если лапп `is_startup_active`, иначе при первом запросе.
  - Смена `autoload` инстанс не трогает и действует со следующего старта.
  - Остановка — это `None` в `instance` под его `Mutex`, так что идущий запрос она дождётся.
  - Старый `update_lapp_settings` при выключении инстанс не останавливал; здесь это сознательное отличие.
- **`open_lapp`** — `async` и `Result<(), String>`.
  - `async` обязателен: в синхронной команде `WebviewWindowBuilder::build` на Windows зависает (документация tauri).
  - Отказ неизвестному лаппу (`laplace` в их числе: лаунчера в реестре нет) и выключенному.
  - Ошибку сборки окна вернуть во фронт вместо `expect("Failed to open lapp")`.
  - `todo!()` в `on_download` на прочих вариантах `DownloadEvent` (enum `non_exhaustive`) заменить на лог.

Команды регистрируются в `invoke_handler`. Фронт их пока не вызывает.

Готово, когда тесты показывают:

- `get_lapps` возвращает лаппы фикстуры по имени, с решениями пользователя и действующими правами;
- `update_lapp` с `enabled = false` пишет `lapps/config/<name>.toml`, не трогает манифест, останавливает работающий инстанс, а следующий запрос получает `NotEnabled`;
- отзыв `invoke_back` останавливает инстанс, следующий запрос получает `PermissionDenied`;
- изменение, которое ничего не меняет, файл не пишет и инстанс не трогает.

`cargo make lint` и `cargo make test` проходят.

---

Сделано:

- `laplace-settings`: `LappInfo { name, manifest, user_settings, effective_permissions, is_running }`
  рядом с `LappUserSettings`.
- `LappsManager` (`crates/app/src/lapps.rs`):
  - `lapps()` — `LappInfo` всех лаппов по имени;
  - `update_lapp(UpdateQuery)` — изменение в копии решений под write-блокировкой лаппа,
    `LappUserSettings::update` (пишет файл, только если что-то применилось), затем копия становится
    решениями реестра — неудачная запись реестр не меняет. `enabled = false`, `allow_permission` или
    `deny_permission` в применённой части останавливают инстанс (`None` под его `Mutex`);
  - `check_open(name)` — `NotFound` / `NotEnabled` для `open_lapp`;
  - `Lapp` хранит `user_config_path`; новый вариант `LappError::SaveUserSettings`.
  - `Lapp::is_running` берёт блокировку инстанса через `try_lock` и занятый инстанс считает
    запущенным: иначе `get_lapps` ждал бы окончания идущего вызова back-а.
- Команды (`crates/app/src/api/laplace.rs`), зарегистрированы в `invoke_handler`:
  - `get_lapps` — синхронная;
  - `update_lapp` — `async` через `spawn_blocking`, ошибка в строку только здесь;
  - `open_lapp` — `async`, `name: String`, `Result<(), String>`: отказ по `check_open`, ошибка сборки
    окна вместо `expect`, `todo!()` в `on_download` заменён на лог.
- Фронт лаунчера: `api::open_lapp` идёт через `try_invoke` и ошибку пишет в лог — иначе отклонённый
  промис бросил бы исключение в wasm.
- Тесты (`crates/app/tests/lapps.rs`, всего 13): список по имени с решениями, действующими правами и
  `is_running`; выключение (файл записан, манифест не тронут, инстанс остановлен, `NotEnabled`); отзыв
  `invoke_back` (`PermissionDenied`); запрос без изменений (файла нет, инстанс работает);
  `update_lapp` неизвестного лаппа — `NotFound`.
- Документация: «Окна» и «Реестр лаппов» в `docs/architecture.md`, L3, L5, U1 в
  `docs/requirements.md`, «Known rough edges» в `AGENTS.md`.

`cargo make lint` и `cargo make test` проходят. Окно приложения не запускалось, команды через `invoke`
вручную не проверялись.
