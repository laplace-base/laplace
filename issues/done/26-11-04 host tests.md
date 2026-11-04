Этап 0.4 плана миграции (`26-10-01 migration plan.md`).

Сделать каркас тестов хоста. Нужен тест-хелпер, который находит собранный `echo-back.wasm` и прогоняет через `LappState` запрос без Tauri. `cargo make test` должен собирать компоненты до запуска тестов, а CI — устанавливать `cargo-component` и цель `wasm32-wasip1`.

Готово, когда в CI проходит тест «echo отвечает на GET».

---

Сделано:

- `crates/app/tests/lapps.rs`: хелпер `state_with(&[имена])` собирает `State` из back-ов в
  `lapps/back/`. Тесты `echo_responds_to_get` и `request_to_missing_lapp_fails`.
- `process_http` стал публичным: это путь `handle_http` без Tauri, включая конверсию типов.
- `Makefile.toml`: задача `test-backs` (`back` с `APP_NAME=echo`), от которой зависит `test`.
- CI: системные библиотеки Tauri (webkit2gtk, gtk3, soup3, dbus), цель `wasm32-wasip1`,
  `cargo-component`, пустой `lapps/front` — без него `generate_context!` не компилируется.
  Без библиотек и этого каталога `laplace-app` в CI не собирался вовсе.
- Попутно: после `fix: host cleanup` clippy падал на неиспользуемом импорте `tauri::http` в
  `lib.rs`; импорт и ставшая ненужной зависимость `mime` удалены.

Проверено прогоном `cargo make lint` и `cargo make test` в чистой копии без `target/` и `lapps/`,
как в CI: компонент собирается, оба теста проходят. Сам workflow на GitHub не запускался; список
apt-пакетов проверен только тем, что на машине разработчика они стоят.

---

 Задача 26-11-04 сделана: тест «echo отвечает на GET» проходит в чистой копии проекта без  target/  и  lapps/ , как в CI. Сам workflow на GitHub я не запускал — это последняя непроверенная часть.

 Тесты. Они лежат в lapps.rs (crates/app/tests/lapps.rs). Хелпер  state_with(&["echo"])  загружает back-и из  lapps/back/  в  State . Запросы идут через  pr
 ocess_http  — это то же, что делает  handle_http , только без Tauri. Ради этого я сделал  process_http  публичной. Тестов два:

   •  echo_responds_to_get  — проверяет статус 200 и точное тело ответа;
   •  request_to_missing_lapp_fails  — проверяет ошибку «Lapp 'missing' is not loaded».

  Makefile.toml . Добавил задачу  test-backs , теперь  test  от неё зависит. Она собирает компонент echo через  cargo component  и копирует его в  lapps/back/echo/ , даже если каталога ещё нет. Обычный  cargo t
 est  без собранного компонента падает с подсказкой запустить  cargo make test .

 CI. Кроме  wasm32-wasip1  и  cargo-component , пришлось добавить ещё две вещи. Без них  laplace-app  в CI вообще не собирался, и это было до моих изменений:

   • системные библиотеки Tauri: webkit2gtk-4.1, gtk3, soup3, dbus;
   • пустой каталог  lapps/front .  tauri::generate_context!  не компилируется, если каталога из  frontend-dist  нет.

 Clippy падал на текущем  HEAD . После коммита  fix: host cleanup  в  lib.rs  остался неиспользуемый импорт  tauri::http . Ваш  cargo make lint  проходил только по старому кэшу, а чистая копия это показала.
 Импорт я убрал, вместе с ним — зависимость  mime , которой после чистки никто не пользуется.

 Набор apt-пакетов я проверил только тем, что локально они установлены. Если в CI чего-то не хватит, ошибка появится на шаге lint.

  AGENTS.md . Описал, как устроены тесты хоста и что  test-backs  собирает back-и. Убрал устаревшее упоминание обработчика  app://  и добавил пункт про обязательный  lapps/front .

 Итоги записал в файл задачи после  --- . Временную копию в  /tmp  удалил. Ничего не коммитил.
