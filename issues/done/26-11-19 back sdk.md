Этап 2.6 плана миграции (`26-10-01 migration plan.md`).

Сейчас каждый back сам себе SDK. Его `bindings.rs` генерирует `cargo component build` под мир из `[package.metadata.component.target]`, и файл лежит в репозитории. Код back-а работает с типами WIT напрямую: сам разбирает путь (`request.uri.path_and_query`), сам собирает `Response`. Для `init` back выбирает другой мир (`back-init`). Отсюда три неудобства:
- в каждом лаппе одна и та же обвязка;
- закоммиченный `bindings.rs` устаревает при любой правке WIT, и `cargo make lint` падает, пока back-и не пересобраны;
- на этапе 3 к этому добавятся обёртки над импортами хоста в каждом лаппе.

Что сделать:

- **Крейт `crates/back-api`** (`laplace-back-api`). Привязки `wit_bindgen::generate!` по `../../wit` для мира `back-init`, с `pub_export_macro: true`, чтобы лаппы не генерировали привязки сами. `wit-bindgen` — в `[workspace.dependencies]`, версия не моложе недели. Сгенерированные типы наружу не торчат: лапп видит API SDK.
- **API лаппа.** Трейт вида:
  ```rust
  pub trait Lapp {
      fn handle(request: http::Request<Vec<u8>>) -> http::Response<Vec<u8>>;
      fn init(previous_version: Option<String>) -> Result<(), String> { Ok(()) }
  }
  ```
  и макрос `laplace_back_api::export!(MyLapp)`, который реализует для него сгенерированные `Guest` обоих интерфейсов. SDK всегда экспортирует `lifecycle`: у back-а без своего `init` хост вызывает пустой. Тогда мир один, и выбирать `back`/`back-init` лаппу не нужно. Мир `back` в WIT остаётся для back-ов без SDK.
- **Конверсия в типы `http`.** `Request`/`Response` WIT ↔ `http::Request<Vec<u8>>`/`http::Response<Vec<u8>>`: метод, URI из `scheme`/`authority`/`path-and-query`, версия, заголовки, тело. Хост делает обратное в `crates/app/src/api/http.rs` (`mod convert`). Расхождения в правилах — версия строкой, заголовки с невалидными именами — описать в документации SDK и согласовать с хостом.
- **Роутинг по пути.** Небольшой `Router`: маршрут — метод и путь относительно `/<name>/api` (префикс SDK снимает сам, имя берёт из пути запроса), обработчик — функция от запроса. Нет маршрута — `404`, есть путь, но не тот метод — `405`. Без параметров в пути и без зависимостей вроде `matchit`: если понадобится больше, расширить потом.
- **Обёртки над импортами** этапа 3 (`database::query`, …) — не в этой задаче. Записать в плане, пункт 3, что они идут в `laplace-back-api` с типами SDK, а не WIT.
- **Сборка без `cargo component`.** С `wit_bindgen::generate!` компонент собирается целью `wasm32-wasip2` (`cargo build --target wasm32-wasip2`), и `cargo-component` больше не нужен: `bindings.rs` не генерируется и не коммитится, а lint не зависит от сборки back-ов. Перевести `build-back`/`deploy-back` в `Makefile.toml` и CI (`.github/workflows/ci.yml`: цель и установка `cargo-component`). Цель `wasm32-wasip2` у тулчейна сейчас не установлена — добавить в `AGENTS.md` и CI. Проверить, что хост (WASI p2, `add_to_linker_sync`) запускает компонент `wasm32-wasip2` так же, как `wasm32-wasip1` с адаптером, и что вывод в stdout/stderr (26-11-17) работает.
- **echo и stress на SDK.** echo — через `Router` (`GET /hello` и любой путь — эхо как сейчас), stress — с `init` из трейта. `bindings.rs` и `[package.metadata.component]` удаляются.
- **Документация.** `docs/architecture.md` — раздел про back и SDK; `AGENTS.md` — сборка back-ов, цель `wasm32-wasip2`, таблица крейтов; план — пункт 2.6 и сборка в 7.5.

Готово, когда:

- юнит-тесты `laplace-back-api` на хост-цели: конверсия запроса и ответа в обе стороны, `Router` (маршрут, `404`, `405`, снятие префикса `/<name>/api`). Код конверсии и роутера не должен зависеть от сгенерированных функций импорта, чтобы собираться и проверяться вне wasm;
- echo и stress собраны под `wasm32-wasip2` через SDK, их `bindings.rs` удалены, тесты в `crates/app/tests/lapps.rs` проходят без изменений по смыслу. Ответ echo может измениться, если меняется формат — тогда поправить тесты;
- `cargo make lint` и `cargo make test` проходят, `cargo-component` в задачах `Makefile.toml` и в CI не используется.

---

Сделано:

- `crates/back-api` (`laplace-back-api`): `bindings` — `wit_bindgen::generate!` для `back-init` с
  `pub_export_macro` и `default_bindings_module`; трейт `Lapp` (`handle` на `http::Request<Vec<u8>>`,
  `init` по умолчанию `Ok(())`); макрос `export!`, который заводит скрытый тип с реализациями `Guest`
  `handler` и `lifecycle` и вызывает сгенерированный `export_bindings!`.
- `convert`: WIT ↔ `http` — версия строкой по правилам хоста, неизвестная — HTTP/1.1; заголовок с
  невалидным именем или значением отбрасывается; неразбираемый URI — `/`; абсолютный URI собирается
  из `scheme`/`authority`, если они есть.
- `Router`: точный путь относительно `/<name>/api` (`api_path`), `404`/`405`, `fallback`, обработчики —
  `fn`-указатели.
- `wit-bindgen` 0.57 (уже был в `Cargo.lock` через `wasip2`) без `async`; `wit-bindgen-rt` убран.
- echo — `Router::new().fallback(echo)`, ответ прежний; stress — `init` из трейта, маршруты через
  `api_path`. `bindings.rs` и `[package.metadata.component]` удалены.
- `build-back`: `cargo build --target wasm32-wasip2`, `deploy-back` — из `target/wasm32-wasip2`;
  CI ставит `wasm32-wasip2` и не ставит `cargo-component`. Компонент wasip2 импортирует WASI 0.2.9,
  хост линкует его через `add_to_linker_sync`; тесты вывода в лог проходят.
- Тесты SDK: 4 на конверсию, 3 на роутер — на хост-цели.
- `docs/architecture.md`, `AGENTS.md` (таблица крейтов, сборка back-ов, инструменты), `README.md`
  (цель и `cargo-component`), план (2.6 и обёртки импортов в этапе 3).

`cargo make lint` и `cargo make test` проходят.

---

 Задача выполнена: back-и пишутся на SDK  laplace-back-api  и собираются обычным  cargo build  под  wasm32-wasip2 , без  cargo component .  cargo make lint  и  cargo make test  проходят. CI я тоже поправил, но сам он не запускался.

 SDK ( crates/back-api ):

   • Привязки. Привязки к миру  back-init  генерируются внутри SDK, и сгенерированные типы WIT лаппу не видны.
   •  Lapp  и  export! . Лапп реализует трейт  Lapp :  handle  принимает и возвращает типы  http , а у  init  есть пустая реализация по умолчанию. Экспортирует его макрос  laplace_back_api::export!(MyLapp) , и
     back на SDK всегда экспортирует  lifecycle .
   • Конверсия WIT ↔  http . Версия передаётся строкой, как у хоста; неизвестная считается HTTP/1.1. Заголовок с недопустимым именем или значением отбрасывается, неразбираемый URI заменяется на  / .
   •  Router . Ищет маршрут по методу и точному пути относительно  /<name>/api , префикс снимает функция  api_path . Нет маршрута —  404 , есть путь, но не тот метод —  405 ; можно задать обработчик по
     умолчанию ( fallback ).
   • Тесты. Четыре на конверсию и три на роутер. Они идут на обычной цели, без wasm.

 Зависимости.  wit-bindgen 0.57  уже был в  Cargo.lock  через крейт  wasip2 , новой версии я не тянул. Подключён без  async ,  wit-bindgen-rt  убран.

 echo и stress.

   • echo — через  Router  с обработчиком по умолчанию, ответ прежний, тесты не менялись.
   • В stress  init  теперь из трейта, а маршруты разбираются через  api_path .
   • Их  bindings.rs  и секции  [package.metadata.component]  удалены.

 Сборка.

   •  build-back  теперь  cargo build --target wasm32-wasip2 ,  deploy-back  берёт файл из  target/wasm32-wasip2 .
   • CI ставит цель  wasm32-wasip2  и больше не ставит  cargo-component .
   • Хост запускает такой компонент без изменений: он импортирует WASI 0.2.9, а тесты на вывод в лог и на панику проходят.
   • Lint больше не зависит от того, пересобраны ли back-и.

 Документация. Обновлены  docs/architecture.md ,  AGENTS.md  (таблица крейтов, сборка, инструменты),  README.md  (цель и  cargo-component ) и план — пункт 2.6 и запись о том, что обёртки над импортами этапа 3
 пойдут в SDK. Итоги дописал в задачу после  --- .
