Этап 2.5 плана миграции (`26-10-01 migration plan.md`).

Сейчас контракт back-а с хостом — пакет `laplace:http` без версии в одном файле `wit/http.wit`. В нём интерфейсы `types`, `by-client-handler`, `lifecycle` и миры `http` и `http-init`. Название уже не отражает содержимое: на этапах 3–5 в пакет добавятся импорты файлов, БД, исходящего HTTP, push и P2P. А без версии хост не сможет отличить back, собранный под старый контракт, от back-а под новый — ошибка будет невнятной, на линковке.

Что сделать:

- **Пакет `laplace:lapp@0.1.0`** в каталоге `wit/`, по файлу на интерфейс: `types.wit`, `handler.wit`, `lifecycle.wit` и `worlds.wit` с мирами. Последующие интерфейсы этапов 3–5 добавляются туда же отдельными файлами.
- **Имена.**
  - `by-client-handler` → `handler`: других обработчиков нет, а «by client» — наследие старой реализации.
  - Миры `http`/`http-init` → `back`/`back-init`. Не `lapp`: `bindgen!` назвал бы структуру мира `Lapp` и `LappPre`, а это совпадает с `crate::lapps::Lapp`.
  - `back-init` = `include back` + `export lifecycle`, как сейчас.
- **Импорты возможностей.** Записать в `worlds.wit` правило, что импорты этапов 3–5 объявляются в мире `back`. Хост линкует их всегда, без права — заглушкой. `wit-component` оставляет у компонента только те импорты, которые его код действительно вызывает, поэтому back без БД не импортирует `database`. Проверить это утверждение на текущей сборке (`wasm-tools component wit lapps/back/echo/echo-back.wasm`) и записать результат.
- **Хост.**
  - `bindgen!` — `path: "../../wit"`, `world: "back"`; `Http`/`HttpPre` → `Back`/`BackPre`.
  - Пути типов — `exports::laplace::lapp::types`.
  - `LIFECYCLE_INTERFACE` — `laplace:lapp/lifecycle@0.1.0`: имя экспорта у компонента содержит версию.
  - Сообщения ошибок «Failed to link the Http world» и подобные — под новые имена.
  - Модуль `crate::api::http` остаётся, это команда `handle_http`. Переименовывается только мир.
- **Несовместимый back.** Back, собранный под `laplace:http`, не линкуется. Ошибка `LappError::Start` должна говорить об этом понятно: например, проверить экспорт `laplace:lapp/handler@0.1.0` у компонента при компиляции и, если его нет, сообщить «back is built for another contract: expected laplace:lapp/handler@0.1.0». Совместимость по semver (back `0.1.x` у хоста `0.1.y`) wasmtime для экспортов не делает. Записать в `docs/architecture.md`, что смена минорной версии пакета до 1.0 ломает собранные back-и.
- **Back-и.** echo и stress — `[package.metadata.component.target] world = "back"` / `"back-init"`. Пути в `lib.rs` — `bindings::exports::laplace::lapp::handler::Guest`. `bindings.rs` перегенерируется `cargo component build`, поэтому перед `cargo make lint` нужна сборка back-ов: lint проверяет закоммиченный `bindings.rs`.
- **Документация.** `docs/architecture.md` (таблица файлов, раздел про `init`), `AGENTS.md` (описание back-а, «exports the `laplace:http` WIT world»), план — пункт 2.5 отметить, а в 2.6 SDK опирается на `laplace:lapp`.

Готово, когда:

- `wit/http.wit` заменён пакетом `laplace:lapp@0.1.0`, echo и stress собраны под него;
- тест: back без экспорта `laplace:lapp/handler@0.1.0` получает `LappError::Start` с понятным сообщением. Подойдёт любой компонент без нужного экспорта, например собранный под старый пакет и сохранённый как тестовый файл, или минимальный компонент, созданный в тесте из WAT (`Component::new` с текстом);
- остальные тесты проходят без изменений по смыслу;
- `cargo make lint` и `cargo make test` проходят.

---

Сделано:

- `wit/http.wit` → `wit/{types,handler,lifecycle,worlds}.wit`, пакет `laplace:lapp@0.1.0`;
  `by-client-handler` → `handler`, миры `back` и `back-init`.
- Хост: `bindgen!` по каталогу `wit` и миру `back`, `Back`/`BackPre`, `laplace_lapp_handler()`;
  `HANDLER_INTERFACE`, `LIFECYCLE_INTERFACE` с версией. `Lapp::compiled` до линковки проверяет экспорт
  `laplace:lapp/handler@0.1.0` и иначе отказывает: «The backend is built for another contract».
- echo — `world = "back"`, stress — `"back-init"`, `bindings.rs` перегенерированы.
- Проверено: импорт, объявленный в мире `back`, но не вызываемый кодом (временный `probe` в echo),
  в собранный компонент не попадает (`wasm-tools component wit`).
- Тест `backend_of_another_contract_is_not_started` — компонент из WAT с экспортом старого
  `laplace:http/by-client-handler`.
- `docs/architecture.md`, `AGENTS.md`, план (2.5).

`cargo make lint` и `cargo make test` проходят.

---

 Задача выполнена: контракт back-а теперь — пакет  laplace:lapp@0.1.0 .  cargo make lint  и  cargo make test  проходят.

   • WIT. Вместо  wit/http.wit  в  wit/  четыре файла:  types.wit ,  handler.wit ,  lifecycle.wit  и  worlds.wit . Интерфейс  by-client-handler  переименован в  handler , миры  http / http-init  — в  back / bac
     k-init .
   • Хост.  bindgen!  теперь читает каталог  wit  с миром  back , в коде  Back / BackPre . Имена экспортов  handler  и  lifecycle  указаны с версией.
   • Несовместимый back. При компиляции хост проверяет, что у компонента есть экспорт  laplace:lapp/handler@0.1.0 . Если его нет, back не запускается, а  LappError::Start  сообщает «The backend is built for
     another contract».
   • Back-и. echo собирается на мире  back , stress — на  back-init , их  bindings.rs  перегенерированы.
   • Проверка импортов. Временно добавил в мир  back  импорт  probe , которого echo не вызывает, и собрал echo. По  wasm-tools component wit  в компоненте остались только импорты WASI и экспорты  types  и  hand
     ler  — неиспользуемый импорт мира в него не попадает. Значит, импорты этапов 3–5 можно объявлять в общем мире  back . Временный импорт я убрал, echo пересобрал.
   • Тест  backend_of_another_contract_is_not_started . Back в нём — минимальный компонент на WAT, который экспортирует старый  laplace:http/by-client-handler . Хост принимает компонент и в текстовом виде,
     поэтому он записывается прямо в  old-back.wasm .
   • Документация.
     •  docs/architecture.md : записано, что версия входит в имена экспортов и сопоставления по semver нет, так что до 1.0 любая смена версии пакета ломает собранные back-и.
     •  AGENTS.md : после правки WIT нужно пересобрать back-и до  cargo make lint .
     • В плане пункт 2.5 отмечен как сделанный.

  wit/http.wit  удалён через  git rm , а новые файлы в  wit/  ещё не добавлены в git. Итоги дописал в файл задачи после  --- . Ничего не коммитил
