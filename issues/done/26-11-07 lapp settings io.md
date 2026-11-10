Этап 1.3 плана миграции (`26-10-01 migration plan.md`). Чтение манифеста и решений пользователя.

В `laplace-settings` есть только схемы — ни манифест `lapps/back/<name>/manifest.toml`, ни решения пользователя `lapps/config/<name>.toml` (раскладка 1.1) читать и писать некому. Манифест хост только читает, пишет только решения пользователя. Поэтому `FileSettings` из старого `laplace` переносится не дословно: там `update` переписывал сам манифест, теряя комментарии автора, а следующее обновление лаппа затирало решения пользователя.

Что сделать:

- **Чтение манифеста.** Функция `load` из старого `FileSettings` (`../old/laplace/laplace_server/src/lapps/settings.rs`): читает `manifest.toml` через `toml` и проставляет `lapp_name` — в файле имени нет, оно берётся из каталога. Записи манифеста нет.
- **Решения пользователя.** В новый тип `LappUserSettings` добавить `data_dir` (необязательное, по умолчанию `data_root/<name>/`).

  Файла нет — значения по умолчанию: `enabled = true`, `autoload = false`, `allowed_permissions.front = ['invoke_back']` если требуется, `title` и `description` совпадают с указанными в манифесте. Лапп открывается, но прав у него нет, кроме вызова собственного бека.
- **Каталог данных лаппа.** Функция, которая его вычисляет:
  - `data_dir` не задан — `data_root/<name>/`;
  - абсолютный путь берётся как есть;
  - относительный считается от `data_root/<name>/`.

  Каким будет каталог, решает пользователь, и платформа его не ограничивает: это может быть и корень файловой системы, и каталог, общий для нескольких лаппов. В `UpdateQuery` `data_dir` пока нет — пользователь правит файл сам, и новое значение действует после перезапуска платформы.
- **`load` / `save` / `update` для решений пользователя.**
  - `update` применяет `UpdateQuery`, как старый `FileSettings::update`, и обнуляет в ответе поля, которые ничего не изменили. `allow_permission` для разрешения не из `required` манифеста не применяется (кроме `LappsIncoming` и `LappsOutgoing`).
  - `save` пишет атомарно: во временный файл рядом и затем переименование. Каталог `config_root` создаётся при первой записи.
  - Ошибки — `LappSettingsError` на `thiserror`, как в старом.
- **Действующие права** — `(allowed + LappsIncoming + LappsOutgoing) ∩ required`. Если обновление лаппа добавит разрешение в `required`, оно не выдаётся само. Если уберёт — выданное перестаёт действовать (кроме `LappsIncoming` и `LappsOutgoing`). Функция вычисления — здесь, проверка — в 1.4.
- **`UpdateQuery`** с тестами сериализации из `../old/laplace/laplace_common/src/api/update.rs`: поля `lapp_name`, `enabled`, `autoload`, `allow_permission`, `deny_permission`, билдеры, `is_applied`. Вместо `#[skip_serializing_none]` из `serde_with` (его в workspace нет) — `#[serde(skip_serializing_if = "Option::is_none")]` на полях. `UpdateRequest` и `Response` старого HTTP API не переносятся: Tauri сам сериализует аргументы и результаты команд.
- **`is_lapp_startup_active`** из старого (`!is_main(name) && autoload && enabled`) теперь берёт значения из решений пользователя. Константа `main_name = "laplace"` и `is_main`.
- **Зависимости.** `toml` добавить в `[workspace.dependencies]`; `thiserror` там уже есть, `serde_json` для тестов — тоже.

Решить, где живут типы. `LappSettings`, `LappUserSettings` и `UpdateQuery` нужны и фронту лаунчера (этап 6: ответ `get_lapps`, аргумент `update_lapp`). Сейчас `laplace-front` от `laplace-settings` не зависит, а `laplace-settings` тянет `config`, который фронту не нужен. Варианты:

- собирать `laplace-settings` и под `wasm32-unknown-unknown`, а `config` (`LaplaceSettings::new`) и файловый ввод-вывод убрать за feature, которую включает только хост;
- положить общие типы в `laplace-common-api`.

Что бы ни выбрали, крейт с этими типами должен собираться под `wasm32-unknown-unknown`.

Готово, когда проходят юнит-тесты:

- чтение манифеста;
- `load` / `save` / `update` решений пользователя, включая отсутствующий файл и `allow_permission` вне `required`;
- действующие права;
- каталог данных: по умолчанию, абсолютный, относительный;
- сериализация `UpdateQuery`.

Крейт с общими типами собирается под `wasm32-unknown-unknown`.

Внесены исправления и дополнения в документацию docs/.

---

Сделано:

- Типы остались в `laplace-settings`: `config`, `toml`, `thiserror` и всё, что читает и пишет файлы,
  — за feature `io`, которую включает `laplace-app`. Без неё крейт собирается (и проходит clippy)
  под `wasm32-unknown-unknown`, так что фронт лаунчера сможет взять типы оттуда же. `toml = "1.1"`
  добавлен в workspace (та же версия уже была в `Cargo.lock` через `config`).
- `lapp::file` (`io`): `LappSettings::load(name, path)` — только чтение манифеста;
  `LappUserSettings::load(path, manifest)` — нет файла → `LappUserSettings::new(manifest)`, файл при
  этом не создаётся; `save` — во временный `.<name>.toml.tmp` рядом и `rename`, каталог создаётся;
  `update(query, manifest, path)` — `apply` и запись, только если что-то изменилось;
  `LappSettingsError { Io, Deserialize, Serialize }`.
- `LappUserSettings`: `application.data_dir: Option<PathBuf>` и `data_dir(data_root, name)`
  (абсолютный — как есть, относительный — от `data_root/<name>/`); `new(manifest)` — `enabled = true`,
  `autoload = false`, `title`/`description` из манифеста, `invoke_back` в `allowed_permissions.front`,
  если требуется. `Default` у `ApplicationUserSettings` тоже даёт `enabled = true`, чтобы файл без
  ключа `enabled` читался так же. `effective_permissions(manifest)`, `apply(query, manifest)`,
  `allow`/`deny` по общему `Permission`.
- `access::Permission { Front, Back }` (в JSON — `{"front":"invoke_back"}`) для `UpdateQuery`, где
  одно имя (`http`, `websocket`, `lapps_outgoing`) есть у обеих половин; `is_implicit` у всех трёх
  типов — `lapps_incoming`/`lapps_outgoing`.
- `lapp::update::UpdateQuery` с билдерами и `is_applied`; `PermissionsSettings::contains`.
- `MAIN_NAME`, `is_main`, `LappSettings::is_lapp_startup_active(&user)`;
  `LappsSettings::{manifest_path, user_settings_path}`.
- Тесты: 14 юнит-тестов в `lapp.rs`, `lapp/update.rs`, `lapp/file.rs` (в т.ч. чтение
  `examples/echo/manifest.toml`). Тесты `file.rs` нужны с `io`: `cargo make test` их запускает, потому
  что фичи объединяются по workspace, а `cargo test -p laplace-settings` без `--features io` — нет.
- Документация: таблица разрешений front/back, действующие разрешения и решения пользователя в
  `docs/domain.md`, жизненный цикл там же, L5 и B2 в `docs/requirements.md`, `laplace-settings` в
  `docs/architecture.md`, `AGENTS.md`.

`cargo make lint` и `cargo make test` проходят. Хост новые типы пока не использует — это реестр 1.4.

---

 Задача 26-11-07 выполнена: манифест теперь читается, решения пользователя читаются и пишутся.  cargo make lint  и  cargo make test  проходят, 14 новых юнит-тестов зелёные. Хост новые типы пока не использует —
 это реестр 1.4.

 Где живут типы. Остались в  laplace-settings . Всё, что работает с файлами, и зависимость  config  вынесены за feature  io , которую включает только  laplace-app . Без неё крейт собирается под  wasm32-unknown-
 unknown  и проходит clippy, так что фронт лаунчера сможет взять типы оттуда же. В workspace добавлен  toml = "1.1" ; эта версия уже была в  Cargo.lock  через  config .

 Чтение и запись ( crates/settings/src/lapp/file.rs )

   •  LappSettings::load  только читает манифест, записи манифеста нет.
   •  LappUserSettings::load : если файла нет, возвращает начальные решения, а сам файл не создаёт.
   •  save  пишет атомарно: во временный файл рядом, потом переименование; каталог создаётся при первой записи.
   •  update  применяет запрос и пишет файл, только если что-то изменилось.

 Логика ( crates/settings/src/lapp.rs )

   • Начальные решения ( LappUserSettings::new ) — по условию задачи:
     • лапп включён, без автозагрузки;
     • название и описание взяты из манифеста;
     •  invoke_back  выдан, если манифест его требует.
   • Действующие права — требуемые, которые выданы или неявны. Неявные —  lapps_incoming  и  lapps_outgoing .
   •  apply  выдаёт разрешение, только если оно требуется или неявно, и возвращает только то, что реально изменилось.
   • Каталог данных: если не задан —  data_root/<name>/ ; абсолютный путь берётся как есть; относительный — от  data_root/<name>/ .
   • Добавлены  MAIN_NAME ,  is_main  и  is_lapp_startup_active .

 Решения, которых в задаче не было — проверьте:

   • Общий тип  Permission { Front, Back }  для  UpdateQuery . Имена  http ,  websocket  и  lapps_outgoing  есть и у front, и у back, поэтому одно имя разрешения стало неоднозначным. В JSON запрос теперь
     выглядит как  {"allow_permission":{"front":"invoke_back"}} .
   •  data_dir  в секции  [application]  решений пользователя, а не на верхнем уровне файла — как в старом манифесте.
   •  enabled = true  по умолчанию и внутри файла. Если в существующем файле нет ключа  enabled , лапп считается включённым — так же, как при отсутствии файла.
   • Тесты  file.rs  требуют  io .  cargo make test  их запускает, потому что фичи объединяются по workspace, а  cargo test -p laplace-settings  без  --features io  их пропустит.

 Документация:

   •  docs/domain.md : таблица разрешений с разбивкой по front/back, действующие права, решения пользователя, жизненный цикл.
   •  docs/requirements.md : строки L5 и B2.
   • Описание  laplace-settings  в  docs/architecture.md  и в  AGENTS.md .

 Попутно заметил, не трогал:

   • В  examples/echo/manifest.toml  остались старые ключи:  enabled  и  autoload  в  [application]  и секция  [permissions.front] . Новая схема их молча игнорирует. Удалить?
   • В  lapps/back/laplace/manifest.toml  лежит файл от прошлой сборки, хотя лаунчер манифеста больше не получает. Это результат сборки, он в  .gitignore .

 Итоги дописал в файл задачи после  --- . В  issues/done/  её не переносил и ничего не коммитил.
