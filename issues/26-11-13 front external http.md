Разрешение фронта на внешние запросы. Продолжение 1.7 (`26-11-11 per-lapp assets.md`), связано с 3.3 плана миграции (исходящий HTTP back-а).

Сейчас фронт лаппа может обращаться к любым внешним адресам: `fetch`, XHR, WebSocket, `<img>`, `<script>` и прочее webview выполняет сам, мимо обработчика схемы лаппа. Переходы и новые окна уже закрыты (`on_navigation`, `on_new_window`). Нужно, чтобы внешние запросы делал только фронт лаппа, у которого в действующих правах есть `FrontPermission::Http` (и `FrontPermission::Websocket` для WebSocket), и только к хостам из манифеста.

Механизм — заголовок `Content-Security-Policy`, который `FsStoredAssets::respond` ставит на свои ответы. Перехватить внешние запросы webview можно только на WebView2 и Android, на WebKitGTK и WKWebView такого API нет, а CSP исполняют все четыре движка. Политику из заголовка код лаппа ослабить не может: `<meta>` её только ужесточает.

Что сделать:

- **Политика по умолчанию — без внешних адресов.** Источники — только собственный origin лаппа в обеих формах (`lapp-<name>:` и `http://lapp-<name>.localhost`) и IPC Tauri (`ipc:`, `http://ipc.localhost`), иначе не работает `invoke`. Плюс `'wasm-unsafe-eval'` в `script-src`: без него фронт на wasm-bindgen не запустится. Нужны ли `'unsafe-inline'` для стилей или `'unsafe-eval'` для htmx, выяснить на echo и лаунчере. Закрыть `frame-src`, `object-src`, `form-action`, `base-uri`. Набросок:

  ```
  default-src 'self' lapp-echo: http://lapp-echo.localhost;
  script-src  'self' lapp-echo: http://lapp-echo.localhost 'wasm-unsafe-eval';
  connect-src 'self' lapp-echo: http://lapp-echo.localhost ipc: http://ipc.localhost;
  img-src     'self' lapp-echo: http://lapp-echo.localhost data: blob:;
  frame-src 'none'; object-src 'none'; form-action 'none'; base-uri 'none'
  ```

- **С разрешением `http`.** В `connect-src` и `img-src` добавляются хосты из `network.http.hosts` манифеста (`https://<host>`), при `all` — `https://`. С `websocket` — `wss://` для тех же хостов. Методы (`network.http.methods`) CSP не ограничивает; если их нужно соблюдать, запросы идут через back (3.3).
- **Права из реестра.** Обработчик схемы берёт действующие права и манифест лаппа из `State` (`ctx.app_handle()`), политика собирается функцией от них. Заголовок нужен на HTML-документах и скриптах воркеров, на остальных ответах он ни на что не влияет.
- **Смена прав.** CSP фиксируется при загрузке документа. `update_lapp` при выдаче или отзыве фронтового `http`/`websocket` перезагружает открытое окно лаппа — по аналогии с остановкой back-а.
- **Внешние ссылки** окну лаппа по-прежнему недоступны (`on_navigation`). Открывать их в системном браузере — отдельное решение, в эту задачу не входит.
- Записать модель в `docs/domain.md` (таблица разрешений: `http`/`websocket` фронта) и `docs/architecture.md`.

Проверить на каждой платформе (Linux, Windows, Android, по возможности macOS):

- что `'self'` и явный `lapp-<name>:` в CSP работают для своей схемы так же, как для `http`;
- что echo и лаунчер под политикой по умолчанию работают;
- что `fetch("https://example.org")` из окна echo без разрешения отклоняется, а с разрешением и хостом в манифесте — проходит.

Общие cookie внешних сайтов. Хранилище cookie одно на профиль WebView, поэтому все лаппы с правом `http`, обращающиеся к одному сайту, получают одни и те же его cookie — например, одну сессию. CSP этого не меняет. `credentials: "omit"` выбирает код лаппа, а не хост. Хост может развести cookie только отдельным профилем на лапп (`data_directory` на Linux и Windows, `data_store_identifier` на macOS; на Android такого нет). Решить в задаче: отдельный профиль, запрет фронтам `credentials`-запросов к внешним сайтам (не обеспечивается CSP) или внешний HTTP только через back (3.3), где cookie у каждого лаппа свои.

Ограничение: CSP — фильтр движка, а не песочница; мелкие побочные каналы (например DNS-prefetch) он может не закрыть. Для строгого контроля внешний HTTP остаётся за back-ом.

Готово, когда:

- юнит-тесты на сборку политики: без разрешений, с `http` и списком хостов, с `http` и `all`, с `websocket`;
- ответ схемы на HTML несёт заголовок CSP (тест `FsStoredAssets::respond`);
- ручная проверка выше на Linux и Android;
- `cargo make lint` и `cargo make test` проходят.

---

Сделано:

- `crates/app/src/csp.rs`: `policy(lapp_name, effective permissions, network)` — политика по умолчанию
  из задачи плюс `'unsafe-inline'` в `style-src` (атрибуты `style` компонентов wingy, стили
  индикаторов htmx) и `https://<scheme>.localhost`/`https://ipc.localhost` на случай `useHttpsScheme`.
  `http` открывает `https://<host>` в `connect-src` и `img-src` (при `all` — `https:`), `websocket` —
  `wss://<host>` в `connect-src`. Хост из манифеста проверяется на символы имени хоста, иначе мог бы
  дописать директивы.
- `Lapp::csp`; обработчик схемы в `start` берёт политику из реестра (лаунчеру — без прав) и
  `FsStoredAssets::respond` ставит её на каждый ответ с файлом.
- `update_lapp` при выданном или отозванном фронтовом `http`/`websocket` перезагружает открытое окно.
- Cookie внешних сайтов: решено не разводить профилями (на Android их нет); фронтовое `http` — для
  открытых ресурсов, запросы с сессией — через back (3.3). Записано в `docs/architecture.md`.
- Тесты: 5 на `csp::policy`, заголовок в тесте `FsStoredAssets::respond`.
- Документация: «Внешние запросы фронта» в `docs/architecture.md`, `http`/`websocket` в `docs/domain.md`.

`cargo make lint` и `cargo make test` проходят. Ручная проверка на Linux и Android не выполнялась.

---

 Сделал: окна лаппов теперь получают заголовок  Content-Security-Policy , и внешние запросы фронт может делать только с правом  http  и только к хостам из манифеста.  cargo make lint  и  cargo make test 
 проходят. В окне я это не проверял — нужен ручной тест на Linux и Android, шаги ниже.

 Политика по умолчанию ( crates/app/src/csp.rs ). Фронт обращается только к своему origin в обеих формах и к IPC Tauri. Фреймы, плагины, отправка форм и  base  закрыты. В политику пришлось добавить:

   •  'wasm-unsafe-eval'  — без него не запустится фронт на wasm-bindgen;
   •  'unsafe-inline'  для стилей — компоненты wingy ставят атрибуты  style , htmx вставляет стили индикаторов.

 С правами.

   • Право  http  открывает  https://<хост>  для  fetch  и картинок по хостам из  network.http.hosts  манифеста. Если хосты не перечислены или указано  all  — весь  https: .
   • Право  websocket  открывает  wss://  к тем же хостам.

 Хост в манифесте, не похожий на имя хоста, в политику не попадает. Иначе манифест вида  "evil.org; script-src *"  дописал бы в неё свои правила. На это есть отдельный тест.

 Откуда берутся права. Обработчик схемы берёт действующие права лаппа из реестра при каждом ответе. Лаунчер в реестре не состоит, поэтому получает политику без прав. Когда лаунчер выдаёт или отзывает у лаппа  h
 ttp  или  websocket  ( update_lapp ), открытое окно лаппа перезагружается: политика фиксируется при загрузке страницы.

 Cookie внешних сайтов. В задаче нужно было выбрать, как с ними быть. Профили по лаппам не подходят: на Android их нет. Поэтому решил так, и записал это в  docs/architecture.md : фронтовое  http  — для открытых
 ресурсов, а запросы с сессией и cookie лапп делает через свой back (пункт 3.3 плана). Если хотите другой вариант, скажите.

 Документация. В  docs/architecture.md  новый раздел «Внешние запросы фронта», в таблице разрешений  docs/domain.md  уточнены  http  и  websocket  фронта.

 Как проверить (Linux, затем Android через  chrome://inspect ):

   1.  cargo make run  (на Android —  cargo make -p debug android ). Лаунчер и echo должны работать как раньше. Если что-то не грузится, ищите в консоли окна сообщения  Refused to …  с названием правила:
      значит, политике чего-то не хватает.
   2. В консоли echo выполните  fetch("https://example.org").then(r => console.log(r.status), e => console.log("blocked", e)) . Ожидается  blocked  и отказ CSP в консоли.
   3. Выдайте echo право. В  examples/echo/manifest.toml  добавьте  "http"  в  [required_permissions] front  и секцию:

 
   [network.http]
   hosts = ["example.org"]
 

 Пересоберите ( cargo make echo ). В  lapps/config/echo.toml  (на Android —  Documents/Laplace/lapps/config/echo.toml ) запишите  [allowed_permissions]  с  front = ["invoke_back", "http"]  и перезапустите
 приложение. Повторите  fetch  из шага 2: теперь запрос должен пройти CSP. Если сайт не отдаёт CORS-заголовки, ответ может отклонить уже CORS — это другое сообщение. Запрос к другому хосту, например  fetch("htt
 ps://example.com") , по-прежнему должен блокироваться.

   4. После проверки верните  examples/echo/manifest.toml  как было.

 Итоги дописал в файл задачи после  --- . В  issues/done/  её не переносил и ничего 

---

Нужно доработать settings так, чтобы в манифесте указывались требуемые настройки http-доступа, отдельно для front и back, как с общими permissions, а в пользовательском решении можно было бы активировать эти настройки - указывать те пункты из манифеста, которые пользователь разрешил.

---

Доработка настроек: хосты отдельно для front и back, выбор пользователя.

- Манифест: `[network.http.front] hosts`, `[network.http.back] hosts`, `methods`, `timeout_ms`
  (`HttpSettings { front: FrontHttpSettings, back: BackHttpSettings }`). По умолчанию хостов нет
  (`HttpHosts::new()` — пустой список, а не `all`).
- Решения пользователя: `[allowed_network.http] front`, `back` (`AllowedNetworkSettings`,
  `HttpHostsSettings`), хосты из манифеста или `"all"`.
- `HttpHosts::allowed_by`, `LappUserSettings::effective_http_hosts`, `LappInfo::effective_http_hosts`;
  CSP строится по действующим хостам фронта.
- `UpdateQuery` хостов не меняет: пока только правкой `lapps/config/<name>.toml` и перезапуском.
- Ручная проверка из шага 3 теперь такая: в манифест echo — `"http"` в `required_permissions.front` и
  `[network.http.front] hosts = ["example.org"]`; в `lapps/config/echo.toml` —
  `[allowed_permissions] front = ["invoke_back", "http"]` и `[allowed_network.http] front =
  ["example.org"]`.

---

 Доработал настройки: манифест теперь просит хосты HTTP отдельно для front и back, а пользователь выбирает из них разрешённые.  cargo make lint  и  cargo make test  проходят. В окне не проверял.

 Манифест. Хосты задаются отдельно для каждой половины, как права. У back-а остались методы и тайм-аут: для фронта CSP их всё равно не ограничивает.

 
   [network.http.front]
   hosts = ["cdn.example.org"]
 
   [network.http.back]
   hosts = "all"
   methods = ["get"]
   timeout_ms = 500
 

 По умолчанию хостов нет. Раньше отсутствие секции значило  all  — теперь это значит «ничего».

 Решения пользователя ( lapps/config/<name>.toml ). Пользователь выбирает хосты из манифеста — списком или  "all" , то есть всё, что тот просит:

 
   [allowed_network.http]
   front = ["cdn.example.org"]
   back = "all"
 

 Если манифест сам просит  "all" , пользователь может перечислить любые хосты.

 Действующие хосты считает  LappUserSettings::effective_http_hosts . Это хосты манифеста, которые разрешил пользователь, — и только для той половины лаппа, у которой есть действующее право  http . Иначе список
 пуст. CSP окна теперь строится по действующим хостам фронта. Действующие хосты back-а уже вычисляются, их применит исходящий HTTP back-а (пункт 3.3 плана).

 Тесты. Добавлены: выбор хостов из запрошенных, зависимость от права и от выбора пользователя, разбор новой секции манифеста. В CSP добавлен случай «право есть, хостов нет».

 Документация. Обновлены  docs/architecture.md  и таблица разрешений в  docs/domain.md . В задачу 26-11-13 дописано описание доработки и новые шаги ручной проверки.

 Изменить хосты через  update_lapp  пока нельзя:  UpdateQuery  их не поддерживает. Сейчас это делается только правкой  lapps/config/<name>.toml  и перезапуском приложения. Для лаунчера на этапе 6 понадобятся
 поля вроде  allow_http_host  /  deny_http_host  — добавить их сейчас?

 Ничего не коммитил.

---

Посмотри файл crates/settings/src/lapp.rs, там я переделал представление настроек разрешений. Теперь они задаются пользователем также, как требуются в манифесте. Также я внёс исправления в тесты. Поправь в других местах под эту новую структуру разрешений, где это необходимо.

---

Обновление после новой структуры `LappSettings` (`required` в манифесте, `allowed` в решениях пользователя). Ручная проверка, шаг 3: в манифест echo —

```toml
[required.permissions]
front = ["invoke_back", "http"]

[required.network.http.front]
hosts = ["example.org"]
```

в `lapps/config/echo.toml` —

```toml
[allowed.permissions]
front = ["invoke_back", "http"]

[allowed.network.http.front]
hosts = ["example.org"]
```

---

 В коде под новую структуру  required  /  allowed  править ничего не пришлось: всё уже собирается,  cargo make lint  и  cargo make test  проходят. Устаревшие ключи оставались только в документации, их я
 обновил.

   •  docs/architecture.md , раздел «Внешние запросы фронта». Описано, что манифест и решения пользователя имеют одинаковую форму ( LappSettings ): в манифесте  [required.…] , у пользователя  [allowed.…] .
     Права —  [required.permissions]  /  [allowed.permissions] , хосты —  [required.network.http.front]  /  .back  и  [allowed.network.http.front]  /  .back .
   •  docs/domain.md : в таблице разрешений у  http  указаны новые ключи.
   •  AGENTS.md : в описании манифеста  required_permissions  заменён на секцию  [required] , добавлено, что решения пользователя устроены так же в  [allowed] .
   • Задача 26-11-13: дописан шаг 3 ручной проверки в новом формате — что добавить в манифест echo и в  lapps/config/echo.toml .

 Старый формат в репозитории остался только в Android-копии манифеста echo ( crates/app/gen/android/…/assets/data/lapps/back/echo/manifest.toml ). Это результат сборки: он обновится при следующем  cargo make an
 droid .

 На уже установленном Android  Documents/Laplace/lapps/back/echo/manifest.toml  распакуется заново только после обновления пакета, если включён  overwrite_existing_assets  (в пакете он включён). Файлы решений
 пользователя со старыми ключами  [allowed_permissions]  теперь молча игнорируются. Например, echo потеряет  invoke_back  и начнёт получать отказ. Если такие файлы есть, их нужно переписать под  [allowed.permis
 sions] . Локально каталога  lapps/config/  нет.

---

С текущими настройками выполняю в консоли Echo:
fetch("https://example.org").then(r => console.log(r.status), e => console.log("blocked", e))
Получаю вывод:
[Error] Origin lapp-echo://localhost is not allowed by Access-Control-Allow-Origin. Status code: 200
[Error] Fetch API cannot load https://example.org/ due to access control checks.
[Error] Failed to load resource: Origin lapp-echo://localhost is not allowed by Access-Control-Allow-Origin. Status code: 200 (example.org, line 0)
[Log] blocked – TypeError: Load failed
Почему так? Разрешение вроде бы есть.

---

 Разрешение сработало — запрос отклонил сам сайт  example.org , а не наша политика.

 По сообщениям это видно так. Будь запрос запрещён CSP, WebView его бы вообще не отправил, и в консоли было бы сообщение вида  Refused to connect to https://example.org/ because it does not appear in the connec
 t-src directive of the Content Security Policy . А у вас  Status code: 200 : запрос дошёл до  example.org , и сервер ответил.

 Отказ дал CORS. Запрос из  lapp-echo://localhost  к  https://example.org  — запрос с другого origin. Прочитать ответ такого запроса страница может, только если сервер вернул заголовок  Access-Control-Allow-Ori
 gin , разрешающий этот origin.  example.org  такого заголовка не присылает, поэтому WebView отдаёт ответ коду лаппа как ошибку ( TypeError: Load failed ). Это правило браузера и сервера. Хост на него не
 влияет, и наша политика здесь ни при чём.

 Как проверить, что CSP пропускает запрос:

   1. Тот же запрос в режиме  no-cors  — ответ будет «непрозрачным», но сам запрос выполнится:

 
   fetch("https://example.org", { mode: "no-cors" }).then(r => console.log(r.type, r.status), e => console.log("blocked", e))
 

 Ожидается  opaque 0 . Значит, CSP запрос пропустил.

   2. Запрос к хосту, которого нет в списке, например  fetch("https://example.com") . Ожидается сообщение  Refused to connect … Content Security Policy , а не ошибка CORS. Значит, CSP запрещает запросы к хостам
      не из списка.
   3. Чтобы получить нормальный ответ целиком, нужен сервер, который отвечает с CORS. Например,  httpbin.org  отдаёт  Access-Control-Allow-Origin: * . Добавьте  "httpbin.org"  в  hosts  в  [required.network.htt
      p.front]  манифеста и в  [allowed.network.http.front]  в  lapps/config/echo.toml , пересоберите echo, перезапустите приложение и выполните:

 
   fetch("https://httpbin.org/get").then(r => r.json()).then(console.log, e => console.log("blocked", e))
 

 Вывод для модели разрешений: фронтовое  http  годится только для сайтов и API, которые разрешают CORS, а это обычно публичные API и CDN. Запросы к остальным сайтам лапп должен делать через свой back (пункт 3.3
 плана): там нет ни CORS, ни общих cookie. Если хотите, добавлю это ограничение в раздел «Внешние запросы фронта» в  docs/architecture.md .
