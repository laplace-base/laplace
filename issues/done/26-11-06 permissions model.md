Этап 1.2 и 1.3 плана миграции (`26-10-01 migration plan.md`). Схема манифеста `manifest.toml`. Чтение манифеста и решений пользователя.

Манифест пишет автор лаппа: он приходит в `.lar`, в package assets на Android и копируется при `cargo make`. Значит, всё, что в нём лежит, — заявка автора, а не решение пользователя. Сейчас схема (`crates/settings/src/lapp.rs`) этого не различает: в манифесте лежат `application.enabled`, `application.autoload` и `permissions.allowed`. В итоге лапп сам выдаёт себе права и сам решает, стартовать ли ему вместе с платформой. Так и сделано сейчас в `examples/echo/manifest.toml`: `allowed` в нём совпадает с `required`.

Решено: манифест описывает, пользователь решает. Решения пользователя переезжают в `lapps/config/<name>.toml` (раскладка 1.1, тип и запись — 1.3). В манифесте остаётся:

- `[application]` — `title`, `description`, `tags`, `additional_static_dirs` и новое поле `version` (строка semver; с этапа 7 по нему различаются обновление и откат на старую версию). `enabled` и `autoload` удаляются;
- `[permissions]` — только `required`, то есть что лапп *просит*. `allowed` удаляется;
- `[database]`, `[network]`, `[lapp_requests]` — без изменений.

---

Сделано
