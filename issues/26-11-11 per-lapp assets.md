Этап 1.7 плана миграции (`26-10-01 migration plan.md`).

Ассеты всех окон отдаёт встроенная схема Tauri (`tauri://localhost`, на Windows и Android — `http://tauri.localhost`) из `FsStoredAssets` (`crates/app/src/assets.rs`). Трейт `Assets` не знает, какое окно просит файл, поэтому окно лаппа сейчас может загрузить что угодно из `frontend_assets_root`, в том числе фронты других лаппов.

Хуже того, ассетам доступны файлы и за пределами корня. `AppManager::get_asset` декодирует проценты в пути до того, как строит `AssetKey`, и `..` при этом не отбрасывает. `FsStoredAssets::get` затем склеивает путь с корнем как есть. Точки в URL нормализует сам webview, но закодированный `%2F` он оставляет, поэтому запрос `/..%2F..%2F<путь>` выходит из `frontend_assets_root`. Так фронт лаппа может прочитать любой файл, доступный процессу: решения пользователя, данные других лаппов, файлы пользователя.

Своя URI-схема для фильтрации не нужна. Встроенный обработчик схемы Tauri после чтения ассета вызывает хук окна `WebviewWindowBuilder::on_web_resource_request` и даёт ему переписать ответ (`tauri::protocol::tauri::get_response`). Хук задаётся на каждое окно, `open_lapp` его уже ставит — пока только для лога. Значит, окно знает имя своего лаппа без всякой схемы. Своя схема может понадобиться только задаче 1.8 — ради отдельного origin на лапп. Отдельные origin нужны для хранилища, а доступу к ассетам они ничего не добавляют.

`vendor/` (`frontend_assets_root/vendor/`: htmx, расширение `client-router`, стили wingy) — общие ассеты платформы, а не лапп. Они доступны любому окну: и лаунчеру, и каждому лаппу.

Что сделать:

- **Выход за корень.** `FsStoredAssets::get` отказывает ключу, в котором после корня есть что-то, кроме обычных компонентов пути (`..`, корень, префикс диска). Это закрывает дыру для всех окон, лаунчер включительно.
- **Фильтр окна лаппа.** Функция от имени лаппа и пути запроса, например `is_asset_allowed(lapp_name, path) -> bool`.
  - Путь — `request.uri().path()`, а не строка URI целиком: так он одинаков на всех платформах.
  - Путь декодируется так же, как это делает `get_asset` (crate `percent-encoding`, уже в `Cargo.lock` через tauri).
  - Пропускаются только пути из обычных компонентов, первый из которых — `vendor` (для любого лаппа) или имя самого лаппа.
- **Имя `vendor` зарезервировать**, как `laplace`: константа рядом с `MAIN_NAME`, и `LappsManager::load` пропускает каталог `vendor` в `back_root`. Иначе лапп с таким именем получил бы фронт в `frontend_assets_root/vendor/` поверх общих ассетов, а его окно — метку, которой фильтр открывает общий каталог.
- **Хук в `open_lapp`.** В `on_web_resource_request` при запрещённом пути ответ заменяется на `403` с пустым телом, в лог пишется предупреждение. Хук ловит и загрузку страниц, и `fetch`/XHR, и iframe внутри окна.
- **Окно лаунчера** создаётся из `Tauri.toml`, хука у него нет, и фильтр ему не нужен: это код платформы. От выхода за корень его защищает первый пункт.
- **Новые окна.** Проверить, что делает `window.open` из окна лаппа. Если Tauri открывает окно приложения, у такого окна не будет ни фильтра, ни метки лаппа. Тогда запретить это через `on_new_window` в `open_lapp`.
- **`additional_static_dirs`** не поддерживать и убрать из `ApplicationSettings`. В старом `laplace` поле отдавало каталоги лаппа как статику: notes так раздавал свой `data`. Теперь данные лежат вне корней автора, а `data_dir` пользователь может направить хоть в корень файловой системы. Если раздавать его как статику, фронт получит данные без разрешения `file_read`. Notes (8.3) будет читать файлы через back.

Готово, когда:

- юнит-тесты на фильтр: `/vendor/htmx/2.0.8/htmx.min.js` пропускается для любого лаппа (`echo`, `notes`), `/echo/index.html` — для `echo`; `/laplace/index.html`, `/other/index.html`, `/vendor/..%2Fother/index.html`, `/echo/..%2F..%2Ffile` и `/` — нет;
- тест реестра: каталог `vendor` в `back_root` не регистрируется;
- юнит-тест на `FsStoredAssets`: ключ с `..` не читается, даже если такой файл есть;
- вручную после `cargo make run`: echo открывается и работает, а `fetch("/laplace/index.html")` и `fetch("/..%2F..%2FLaplace.toml")` из консоли окна echo получают отказ (на Android — по возможности тоже);
- `cargo make lint` и `cargo make test` проходят.

---

Сделано:

- `FsStoredAssets::get` отказывает ключу, в котором после корня есть не только обычные компоненты
  пути (`assets::asset_path`), и пишет предупреждение в лог.
- `assets::is_asset_allowed(lapp_name, path)`: путь декодируется (`percent-encoding`, добавлен в
  workspace — версия та же, что уже в `Cargo.lock`), пропускаются только пути из обычных компонентов
  с первым `vendor` или именем лаппа.
- `open_lapp`: хук `on_web_resource_request` при запрещённом пути отвечает `403` с пустым телом и
  пишет предупреждение; `on_new_window` всегда `Deny` с предупреждением, чтобы не зависеть от того, что
  без обработчика делает движок платформы (не проверялось). На Android `on_new_window` не поддерживается, как там
  ведёт себя `window.open`, не проверялось.
- `lapp::VENDOR_NAME`, `lapp::is_reserved`; `LappsManager::load` пропускает зарезервированные имена.
- `ApplicationSettings::additional_static_dirs` удалён.
- Тесты: фильтр и `FsStoredAssets` в `crates/app/src/assets.rs`, `vendor_is_not_a_registered_lapp`
  в `crates/app/tests/lapps.rs`.
- Документация: «Окна» в `docs/architecture.md`, Configuration в `AGENTS.md`.

`cargo make lint` и `cargo make test` проходят. Ручная проверка (`fetch` из консоли окна echo) не
выполнялась, на Android тоже.

---

 Задача 26-11-11 выполнена. Окно лаппа теперь получает только свой фронт и общие  /vendor/** , а наружу из корня ассетов не выходит ни одно окно.  cargo make lint  и  cargo make test  проходят. Руками в окне я
 это не проверял: ни  fetch  из консоли echo, ни запуск на Android.

   • Выход за корень ( crates/app/src/assets.rs ).  FsStoredAssets::get  больше не читает ключ, в котором есть  ..  или другой необычный компонент пути, и пишет в лог предупреждение. Защита касается всех окон,
     лаунчера тоже. Тест подтверждает, что  /../secret.toml  и  /echo/../../secret.toml  не читаются, хотя файл есть.
   • Фильтр окна лаппа. Функция  is_asset_allowed(lapp_name, path)  раскодирует путь так же, как Tauri, и пропускает только пути, начинающиеся с  vendor  или имени лаппа. Для этого в workspace добавлен  percent
     -encoding  — той версии, что уже была в  Cargo.lock .
   • Хук в  open_lapp . На запрещённый путь окно лаппа получает  403  с пустым телом, предупреждение уходит в лог.
   •  window.open  из окна лаппа всегда отклоняется. Без обработчика поведение зависит от движка, и я его не проверял. На Android  on_new_window  не поддерживается, и что там делает  window.open , тоже не
     проверено.
   • Зарезервированные имена. Добавлены  VENDOR_NAME  и  is_reserved .  LappsManager::load  пропускает каталоги  laplace  и  vendor , это проверяет новый тест  vendor_is_not_a_registered_lapp .
   •  additional_static_dirs  удалён из  ApplicationSettings .
   • Документация: раздел «Окна» в  docs/architecture.md , раздел Configuration в  AGENTS.md .

 Проверить вручную можно так: после  cargo make run  открыть echo и в консоли его окна выполнить  fetch("/laplace/index.html")  и  fetch("/..%2F..%2FLaplace.toml") . Оба запроса должны вернуть отказ, а сам echo
 должен работать как раньше.

 Пункт 1.7 плана ( 26-10-01 migration plan.md ) ещё описывает старый подход — своя схема и  additional_static_dirs . Я его не трогал; могу привести в соответствие.

 Итоги дописал в файл задачи после  --- . В  issues/done/  её не переносил и ничего не коммитил.

---

Почему использовался .strip_prefix('/'), а не .trim_start_matches?

---

 Это сделано намеренно:  strip_prefix('/')  снимает ровно один ведущий  / , а лишний разделитель путь не нормализует, а отклоняет. Различие проявляется только на необычных путях. Безопасность держится не на
 нём, а на проверке компонентов, которая идёт сразу после.

 Что происходит в коде. В обоих местах ( is_asset_allowed  и  asset_path  в  crates/app/src/assets.rs ) после снятия префикса путь разбирается на компоненты. Дальше проходит только путь из одних обычных
 компонентов. Поэтому:

   •  strip_prefix('/')  на  //echo/index.html  оставляет  /echo/index.html . Первым компонентом получается корень ( RootDir ), и путь отклоняется.
   •  trim_start_matches('/')  снял бы все слеши, и  //echo/index.html  прошёл бы как  echo/index.html .

 Почему выбран первый вариант. Ведущий  /  — признак того, что путь от корня, и он ожидается ровно один. Лишний слеш в запросе — уже не тот путь, который отдаёт фронт. Фильтру безопаснее отказать, чем гадать,
 что имелось в виду.

 Вторая причина есть только в  is_asset_allowed .  strip_prefix  возвращает  None , если пути без  /  в начале, и функция тогда отказывает. Путь запроса всегда начинается с  / , так что путь без него — тоже
 признак чего-то необычного.  trim_start_matches  не отличил бы  echo/index.html  от  /echo/index.html .

  trim_start_matches  тоже был бы безопасен. Главная опасность — абсолютный путь в  root.join(...) : он заменяет корень целиком.  trim  её тоже убирает, потому что слешей в начале не остаётся. А  ..  в обоих
 вариантах отсекает проверка компонентов. Разница только в том, что делать с  //… : отказать или принять как обычный путь.

 Побочный эффект. Для  //echo/index.html  фильтр откажет, хотя Tauri этот файл, скорее всего, отдал бы.  get_asset  сам снимает один  / , а  AssetKey  снова приводит путь к одному корню; это я понял по коду, на
 практике не проверял. Получается отказ там, где можно было пропустить. Для фронтов это ничего не меняет: они пишут пути с одним слешем ( /echo/dist/… ,  /vendor/… ).

 Если хотите, чтобы фильтр вёл себя ровно как Tauri и принимал лишние слеши, можно перейти на  trim_start_matches('/') . Проверка компонентов при этом остаётся, так что безопасность не пострадает.

---

Как проверить руками, что хуки в on_web_resource_request и on_new_window работают?

---

 Проверять удобнее всего из консоли devtools окна echo, параллельно глядя в лог приложения в терминале.

 Подготовка

   1. Соберите и запустите приложение:

 
   cargo make run
 

 Если всё уже собрано, хватит  cargo make start . Лог пишется в stderr терминала, потому что  log.path  в  Laplace.toml  не задан.  cargo make start  выставляет  RUST_LOG=info , и предупреждения хуков при этом
 видны. Чтобы видеть ещё и строку  Request: …  на каждый запрос, запустите с  RUST_LOG=debug cargo make start .

   2. В лаунчере откройте echo.
   3. Откройте devtools окна echo: правый клик в окне → «Inspect Element». Feature  devtools  у  tauri  включена в workspace, поэтому инспектор есть и в release-сборке. Откройте вкладку «Console».

  on_web_resource_request 

 Выполните в консоли окна echo:

 
   for (const path of [
     "/echo/index.html",
     "/vendor/htmx/2.0.8/htmx.min.js",
     "/laplace/index.html",
     "/..%2F..%2FLaplace.toml",
   ]) {
     fetch(path).then(r => console.log(path, r.status));
   }
 

 Ожидается:

   •  /echo/index.html  и  /vendor/...  —  200 ;
   •  /laplace/index.html  и  /..%2F..%2FLaplace.toml  —  403 .

 В логе на каждый отказ должно появиться  Lapp 'echo' is denied the asset '…' .

 Навигацию хук тоже ловит. После  location.href = "/laplace/index.html"  окно echo должно показать пустую страницу, а не лаунчер. Затем закройте окно и откройте echo заново.

  on_new_window 

 
   window.open("/echo/index.html")
   window.open("https://example.org")
 

 Ожидается, что новое окно не откроется, а в логе появится  Lapp 'echo' is denied opening a new window for '…' . В консоли  window.open  скорее всего вернёт  null ; этого я не проверял.

 Защита  FsStoredAssets  без хука

 У окна лаунчера хука нет, так что проверка в нём показывает отдельно, что  FsStoredAssets  не выходит за корень. Откройте devtools окна лаунчера и выполните:

 
   fetch("/..%2F..%2FLaplace.toml").then(r => r.text().then(t => console.log(r.status, t)))
 

 Ожидается, что содержимого  Laplace.toml  в ответе нет. Код ответа, скорее всего,  500 : ассета нет, и запасного  index.html  в корне тоже нет. В логе должно быть  Asset '…' is rejected: it leads out of the as
 sets root .

 Убедиться, что проверка что-то ловит

 Чтобы не принять «отказ по другой причине» за работающую защиту, можно сравнить с кодом до задачи. Сохраните текущие правки ( git stash ), пересоберите и выполните тот же  fetch  в окне лаунчера. Если там
 придёт содержимое  Laplace.toml , дыра действительно была и правка её закрыла. Потом верните правки через  git stash pop . Этот шаг необязателен: он меняет рабочее дерево.

 Android

 Devtools открываются через  chrome://inspect  в Chrome на компьютере. Нужна debug-сборка  cargo make -p debug android : в release-APK отладка WebView, скорее всего, выключена. Лог смотрите в logcat под тегом  
 Laplace .  on_new_window  на Android не поддерживается, так что для  window.open  там важно увидеть одно: новое окно лаппа не появляется.

---

Результат, который я получаю в первом ручном тесте. Консоль:

```
[Log] /echo/index.html – 200
[Log] /vendor/htmx/2.0.8/htmx.min.js – 200
[Log] /laplace/index.html – 200
[Error] Failed to load resource: the server responded with a status of 500 (Internal Server Error) (../../Laplace.toml, line 0)
[Log] /..%F..%FLaplace.toml – 500
```
Лог:
```
[2026-11-12 13:59:05.902292] WARN [laplace_app_lib::assets] app/src/assets.rs:51: Asset '/../../Laplace.toml' is rejected: it leads out of the assets root
[2026-11-12 13:59:05.902384] DEBUG [tauri::manager] tauri-2.12.5/src/manager/mod.rs:396: Asset `../../Laplace.toml` not found; fallback to ../../Laplace.toml.html
[2026-11-12 13:59:05.902427] WARN [laplace_app_lib::assets] app/src/assets.rs:51: Asset '/../../Laplace.toml.html' is rejected: it leads out of the assets root
[2026-11-12 13:59:05.902455] DEBUG [tauri::manager] tauri-2.12.5/src/manager/mod.rs:403: Asset `../../Laplace.toml` not found; fallback to ../../Laplace.toml/index.html
[2026-11-12 13:59:05.902483] WARN [laplace_app_lib::assets] app/src/assets.rs:51: Asset '/../../Laplace.toml/index.html' is rejected: it leads out of the assets root
[2026-11-12 13:59:05.902504] DEBUG [tauri::manager] tauri-2.12.5/src/manager/mod.rs:410: Asset `../../Laplace.toml` not found; fallback to index.html
[2026-11-12 13:59:05.902540] WARN [laplace_app_lib::assets] app/src/assets.rs:62: Asset '/index.html' not found: filе 'lapps/front/index.html' is not exists
[2026-11-12 13:59:05.902559] ERROR [tauri::manager] tauri-2.12.5/src/manager/mod.rs:418: asset not found: ../../Laplace.toml
```

---

 Ваш результат показывает две вещи. Защита  FsStoredAssets  работает: в логе  is rejected: it leads out of the assets root , файл не прочитан. А хук окна echo на Linux не вызывается вообще. Ни  403 , ни строки
 Request: …  из этого хука в логе нет, хотя debug-уровень включён — строки  tauri::manager  видны.

 Почему хук не вызывается

 Я проверил по исходникам  tauri 2.12.5 ,  tauri-runtime-wry  и  wry 0.57.1 .

   • Обработчик схемы  tauri://  Tauri создаёт при сборке каждого окна. Хук  on_web_resource_request  вшивается внутрь этого обработчика ( manager/webview.rs ,  protocol::tauri::get ).
   • На Linux webkit2gtk регистрирует схемы не на окно, а на  WebContext , и одну схему в контексте можно зарегистрировать только раз. Поэтому  tauri-runtime-wry  пропускает схему, которая в контексте уже есть
     ( lib.rs:5114–5131 : «on Linux the custom protocols are associated with the web context and you cannot register a scheme more than once»).
   • У всех наших окон контекст общий: ключ контекста —  data_directory , а мы его не задаём. Первым создаётся окно лаунчера, и  tauri://  регистрируется с его обработчиком, где хука нет. Обработчик окна echo с
     фильтром отбрасывается. Поэтому  /laplace/index.html  и вернул  200 .

 Отсюда же видно, почему  /..%2F..%2FLaplace.toml  дал  500 , а не  403 :  get_asset  при ошибке выходит через  ?  раньше, чем вызывается хук. Содержимое при этом не утекает. Но и на других платформах для
 отсутствующих ассетов хук не сработает — будет  500  вместо  403 .

 На Windows, macOS и Android схемы регистрируются на webview, так что хук там, скорее всего, работает. Но я это не проверял, а для Linux мой вывод в задаче «своя схема не нужна» оказался неверным.

 Варианты исправления

   1. Свой  WebContext  у каждого лаппа. В  open_lapp  задавать окну  data_directory , например  app_local_data_dir()/webview/<name> . Тогда у лаппа будет свой контекст,  tauri://  в нём зарегистрируется с
      хуком этого окна, и фильтр заработает.
      • Плюсы: правка в несколько строк. Заодно на Linux и Windows разделяется хранилище WebView ( localStorage , IndexedDB, cookies) — это половина задачи 1.8.
      • Минусы: отдельные процессы WebKit/WebView2 на каждый лапп, то есть больше памяти. Фильтр держится на внутреннем поведении Tauri: если когда-нибудь окна снова окажутся в общем контексте, он молча
        пропадёт. На macOS  data_directory  не поддерживается, но там, судя по коду, хук и так задаётся на webview.
   2. Своя схема через  register_asynchronous_uri_scheme_protocol . В её обработчик приходит  UriSchemeContext::webview_label() , поэтому фильтр по метке окна работает на любой платформе, и как регистрируются
      схемы — неважно.
      • Плюсы: фильтр не зависит от контекстов и от порядка создания окон.
      • Минусы: окна лаппов переходят на  WebviewUrl::CustomProtocol .  tauri://  при этом остаётся открытым для явных абсолютных URL: лапп сможет подключить чужой скрипт через  <script src="tauri://localhost/o
        ther/…"> , но не прочитать его через  fetch  — CORS не даст. Чтобы закрыть это совсем, на свою схему пришлось бы перевести и лаунчер, правкой  url  в  Tauri.toml . Работы заметно больше.

 Я бы начал с варианта 1. Он маленький, закрывает дыру на Linux и продвигает задачу 1.8. Его стоит дополнить явной проверкой при сборке окна, чтобы фильтр не потерялся молча. А вариант 2 оставить на случай,
 если 1.8 всё равно приведёт к своей схеме ради отдельного origin. После любого из вариантов нужно повторить ваш ручной тест и сделать то же на Android.

 Какой вариант делать?
