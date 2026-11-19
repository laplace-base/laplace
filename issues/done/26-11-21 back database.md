Этап 3.2 плана миграции (`26-10-01 migration plan.md`). Опирается на контекст инстанса (`InstanceContext`, 3.1). Первая возможность с функциями: в ней решается, как отказывать без права (введение этапа 3 плана).

Сейчас у back-а нет базы данных. В старом Laplace (`../old/laplace/laplace_server/src/lapps/wasm_interop/database.rs`, `laplace_wasm/src/database.rs`) были `execute`, `query` и `query_row` над SQLite (`rusqlite`). Запрос передавался строкой без параметров, так что значения лапп вставлял в SQL сам. Манифест уже умеет `[database] path` (`LappManifestSettings::database`), право `database` есть в `BackPermission`.

Что сделать:

- **WIT** — `wit/database.wit`, интерфейс `laplace:lapp/database@0.1.0`:
  ```wit
  variant value { null, integer(s64), real(f64), text(string), blob(list<u8>) }
  record row { values: list<value> }
  execute: func(sql: string, params: list<value>) -> result<u64, string>;
  query: func(sql: string, params: list<value>) -> result<list<row>, string>;
  query-row: func(sql: string, params: list<value>) -> result<option<row>, string>;
  ```
  Параметры (`?1`, `?2` …) — в отличие от старого, чтобы значения не попадали в текст SQL. Импорт — в мире `back` (`wit/worlds.wit`): back, который БД не вызывает, его не импортирует. Имена колонок (`Column` старого) — по желанию; если нужны, добавить `columns: list<string>` в результат `query`.
- **Хост.**
  - `rusqlite` с feature `bundled` в `[workspace.dependencies]`, версия не моложе недели. `bundled` нужен, чтобы собиралось под Android без системной SQLite.
  - Реализация `Host` для интерфейса, сгенерированного `bindgen!`. Отказ без права `database` — `Err("permission denied: database")`, а как он устроен — заглушки в `Linker` по набору прав или проверка в функциях — выбрать в начале задачи по вариантам из введения этапа 3 плана и записать решение туда и в `docs/architecture.md`.
  - Соединение открывается лениво, при первом вызове, и живёт в данных `Store` до сброса инстанса. Файл БД — `<каталог данных>/<[database].path>`, по умолчанию, например, `database.sqlite`. Путь из манифеста — только относительное имя без `..` и без абсолютных компонентов; иначе `LappError::Start` или ошибка вызова. Каталог данных создаётся и для права `database` без файловых прав, но preopen по-прежнему зависит только от `file_*`.
  - Вызовы синхронные, под дедлайном вызова back-а. Долгий запрос SQLite дедлайн по эпохам не прервёт, это блокирующий вызов хоста: поставить `Connection::busy_timeout` и прерывание через `Connection::get_interrupt_handle` по истечении `call_timeout_ms` или записать ограничение в `docs/architecture.md`.
  - Перенос `to_value`/`to_row` из старого `database.rs`.
- **SDK.** Модуль `laplace_back_api::database`: `execute`, `query`, `query_row` с параметрами, свой тип `Value` с `From` для `i64`, `f64`, `String`, `&str`, `Vec<u8>`, `Option<T>`. Типы WIT наружу не торчат. Вызов вне wasm (тесты SDK) не нужен.
- **Android.** Проверить, что `rusqlite` с `bundled` собирается (`cargo make -p debug android`), а база создаётся в каталоге данных на устройстве.
- **Документация.** `docs/architecture.md` (раздел про возможности back-а), `docs/domain.md` (право `database`), план (3.2).

Готово, когда:

- stress (или тестовый back `examples/todo`, если сразу начинать 8.2) получает маршруты, которые создают таблицу, пишут и читают строку через параметры;
- тесты в `crates/app/tests/lapps.rs`:
  - без права `database` вызов получает `permission denied: database`, но back запускается и отвечает на остальное;
  - с правом создаётся файл БД в каталоге данных, данные переживают перезапуск инстанса;
  - путь `[database].path` с `..` отклоняется;
  - все варианты `value` проходят туда и обратно;
- юнит-тесты конверсий `rusqlite` ↔ `value` на хосте;
- `cargo make lint` и `cargo make test` проходят; сборка под Android проходит.

---

Сделано:

- Отказ без права — проверка в функциях (`WasiStates::check`), импорты линкуются в общий `Linker`
  всегда; записано во введение этапа 3 плана и в `docs/architecture.md`.
- `wit/database.wit` — `execute`, `query`, `query-row` с параметрами; импорт в мире `back`. Back без
  вызовов БД (echo) его не импортирует — проверено `wasm-tools component wit`.
- Хост: `crates/app/src/database.rs` — путь файла (`[database].path`, по умолчанию `database.sqlite`,
  без `..` и абсолютных — `LappError::Start`), ленивое соединение, конверсии `rusqlite` ↔ `value`,
  обработчик прогресса SQLite, обрывающий запрос по дедлайну вызова (`CallDeadline`, ставится вместе с
  эпохой перед каждым вызовом). `InstanceContext::database_path`, `DatabaseHost for WasiStates`.
- `rusqlite` 0.41 (`bundled`, `hooks`): 0.42 и 0.43 моложе недели.
- SDK: `laplace_back_api::database` — `Value` с `From`, `Row`, `Error::is_permission_denied`.
- Пример `examples/todo/{back,common}` вместо маршрутов stress: таблица в `init` (без права — лапп
  стартует и отвечает ошибкой), `list`/`add`/`update`/`delete`/`clear_completed`, JSON, SQL с
  параметрами. Фронт — этап 8.2.
- Тесты: 4 юнит-теста хоста (все варианты `value` туда и обратно, `query`/`query-row`, путь, обрыв
  бесконечного запроса), 3 юнит-теста SDK, 3 интеграционных с todo (без права — `permission denied`,
  лапп работает, файла нет; задачи переживают перезапуск, строка с кавычками и `;` — данные, а не SQL;
  `../todos.db` — отказ).
- `docs/architecture.md`, `docs/domain.md`, `AGENTS.md`, план (введение этапа 3, 3.2, 8.2).

`cargo make lint` и `cargo make test` проходят. Сборка под Android не проверена: в окружении нет
Android SDK.

---

 Задача выполнена: у back-а есть своя база SQLite. Проверяю её, как вы и предложили, на примере todo.  cargo make lint  и  cargo make test  проходят. Сборка под Android не проверена: по  ANDROID_HOME 
 здесь нет Android SDK.

 Отказ без права — проверкой в функциях. Импорты возможностей всегда линкуются в общий  Linker . Каждая функция первой строкой вызывает  WasiStates::check  и без права возвращает  permission denied: database .
 Решение записано во введение этапа 3 плана вместе с обоснованием и отвергнутым вариантом, а также в  docs/architecture.md . Из открытых решений вопрос убран.

 WIT. В  wit/database.wit  три функции:  execute ,  query ,  query-row  — с параметрами  ?1 ,  ?2 , … Импорт объявлен в мире  back . По  wasm-tools component wit  echo его не импортирует, а todo импортирует.

 Хост ( crates/app/src/database.rs ):

   • Файл БД. Путь берётся из  [database].path  манифеста и должен быть относительным, без  ..  — иначе  LappError::Start . Без  path  файл называется  database.sqlite .
   • Соединение. Открывается при первом вызове и живёт до сброса инстанса.
   • Долгие запросы. Дедлайн по эпохам вызов хоста не прерывает. Поэтому SQLite периодически вызывает обработчик прогресса, и тот обрывает запрос по дедлайну текущего вызова back-а.
   • Версия  rusqlite . Взял 0.41 с  bundled  и  hooks : 0.42 и 0.43 вышли меньше недели назад.

 SDK. Модуль  laplace_back_api::database : свой  Value  с  From  для обычных типов и  Row .  Error::is_permission_denied  отличает отказ хоста от ошибки SQLite.

 Пример todo ( examples/todo/back  и  examples/todo/common ):

   • Это порт старого todo на SDK:  list ,  add ,  update ,  delete ,  clear_completed , ответы в JSON. Значения теперь передаются параметрами, а не подставляются в текст SQL.
   • Таблица создаётся в  init . Без права лапп всё равно запускается и на каждый запрос отвечает, почему не работает.
   • У задач теперь есть  id , и старый трюк с  VACUUM  для перенумерации строк не нужен.
   • Фронт остаётся для пункта 8.2. todo собирается в  test-backs .

 Тесты:

   • 4 юнит-теста хоста: все варианты  value  туда и обратно,  query  и  query-row , проверка пути, обрыв бесконечного запроса.
   • 3 юнит-теста SDK.
   • 3 интеграционных теста с todo:
     • без права ответ  permission denied: database , лапп работает, файла нет;
     • задачи переживают перезапуск, а описание с кавычками и  ; DROP TABLE  сохраняется как данные;
     • путь  ../todos.db  отклоняется.

 В тестах используется ваш новый  restart_lapp .

 Обновлены  docs/architecture.md ,  docs/domain.md ,  AGENTS.md  и план (введение этапа 3, пункты 3.2 и 8.2). Итоги дописал в задачу после  --- .

 Сборку под Android нужно проверить у вас:  cargo make -p debug android . Он соберёт SQLite из исходников компилятором NDK.

---

Хорошо бы примеру todo добавить фронт, как в старой реализации, чтобы проверить лапп вручную, при запуске платформы.

---

Сделано: фронт todo — `examples/todo/front` (`todo-front`) на hypertext + wingy + htmx, по образцу echo,
с поведением TodoMVC старого примера (`../old/laplace/examples/todo/client`, Yew):
- новое задание, список с отметкой, правка на месте (пустое описание удаляет задание), удаление,
  фильтры All / Active / Completed, «Mark all», «Clear completed», счётчик оставшихся;
- каждое действие — htmx-запрос `/todo/api/<действие>`; `main.js` передаёт его вместе со значениями
  формы (`event.detail.formData`, в него входят и `hx-vals`) в wasm `router`, тот вызывает back через
  `handle_http` и перерисовывает секцию `#todo` целиком; ошибка действия показывается `Callout` над
  списком; фильтр — состояние окна (`thread_local`);
- задача `cargo make todo` (front + back), todo входит в `examples`/`all`; в лаунчере — в захардкоженном
  списке рядом с echo (настоящий список — 6.1).

Ручная проверка: `cargo make run`, право `database` — в `lapps/config/todo.toml`
(`[allowed.permissions] front = ["invoke_back"]`, `back = ["database"]`); без него todo показывает
`permission denied: database`.

---

 Фронт todo готов, но в окне я его не запускал: здесь приложение падает с segfault ещё до открытия окон (как и раньше). Сборка  cargo make all  и  cargo make lint  /  cargo make test  проходят,  lapps/front/tod
 o  собран.

 Фронт ( examples/todo/front ). Сделан по образцу echo на hypertext + wingy + htmx, поведение — как в старом todo (TodoMVC):

   • поле нового задания и кнопка «Add»;
   • у каждого задания чекбокс, поле описания, которое правится прямо в списке (если очистить его, задание удаляется), и кнопка удаления;
   • фильтры All / Active / Completed, «Mark all», «Clear completed» и счётчик оставшихся;
   • ошибки действий показываются блоком над списком.

 Как работает. Каждое действие — это htmx-запрос  /todo/api/<действие> .  main.js  передаёт путь и значения формы в wasm-функцию  router . Она вызывает back через  handle_http  и перерисовывает секцию  #todo 
 целиком. Фильтр — состояние окна, back о нём не знает.

 Попутно:

   • задача  cargo make todo  (front + back), todo входит в  examples  и  all ;
   • todo добавлен в захардкоженный список лаунчера рядом с echo, настоящий список через  get_lapps  — пункт 6.1;
   •  AGENTS.md , пункт 8.2 плана и итоги в задаче 26-11-21 обновлены.

 Как проверить:

   1. Создайте  lapps/config/todo.toml :

 
   [allowed.permissions]
   front = ["invoke_back"]
   back = ["database"]
 

 Без этого файла todo покажет  permission denied: database  — так тоже стоит один раз проверить.

   2.  cargo make run , в лаунчере откройте «Todo».
   3. Добавьте пару заданий, отметьте одно, поправьте описание (Enter или уход из поля), переключите фильтры, нажмите «Mark all» и «Clear completed», удалите задание крестиком.
   4. Перезапустите приложение: задания должны сохраниться. База — в  lapps/data/todo/todos.db .

 Если что-то в окне не сработает, пришлите вывод консоли окна todo: ошибки там видны по  action … .
