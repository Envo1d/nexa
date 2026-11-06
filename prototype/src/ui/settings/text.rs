// Reference Settings translations from src/i18n/ru.json.
pub(super) fn text(key: &str) -> &'static str {
    match key {
        "appearance" => crate::i18n::text("Внешний вид"),
        "appearanceDescription" => crate::i18n::text("Настройте рабочее пространство."),
        "applicationLogs" => crate::i18n::text("Журналы приложения"),
        "artistDescription" => crate::i18n::text(
            "Использовать автора файла как папку. Сохранённые изменения реорганизуют распределённые файлы при включённой синхронизации.",
        ),
        "artistDirectory" => crate::i18n::text("Папка по автору"),
        "autoScan" => crate::i18n::text("Сканировать при запуске"),
        "autoScanDescription" => {
            crate::i18n::text("Сканировать библиотеку и входящие при каждом запуске Nexa.")
        }
        "autoUpdate" => crate::i18n::text("Проверять при запуске"),
        "autoUpdateDescription" => {
            crate::i18n::text("Проверять обновления при каждом запуске Nexa.")
        }
        "browse" => crate::i18n::text("Обзор"),
        "cache" => crate::i18n::text("Кэш"),
        "cacheClearError" => {
            crate::i18n::text("Не удалось очистить кэш миниатюр. Попробуйте ещё раз.")
        }
        "cacheClearPartial" => {
            crate::i18n::text("Не удалось удалить записи кэша: {count}. Повторите очистку.")
        }
        "cacheCleared" => crate::i18n::text("Кэш миниатюр очищен."),
        "cacheDescription" => crate::i18n::text("Управляйте кэшем миниатюр и местом на диске."),
        "cacheLimit" => crate::i18n::text("Использование диска · Лимит 512 МБ"),
        "cacheUsage" => crate::i18n::text("Использование кэша миниатюр"),
        "cancel" => crate::i18n::text("Отмена"),
        "catalogBackup" => crate::i18n::text("Резервная копия каталога"),
        "catalogBackupDescription" => crate::i18n::text(
            "Сохранение всех записей, авторов, произведений, персонажей, тегов, избранного, порядка файлов и пресетов в JSON. Оригиналы, миниатюры и настройки приложения не включаются. Выберите новое имя файла.",
        ),
        "catalogChooseBackup" => crate::i18n::text("Выбрать копию…"),
        "catalogConfirmReplacement" => crate::i18n::text(
            "Я понимаю, что текущий каталог будет полностью заменён этой резервной копией.",
        ),
        "catalogCreateBackup" => crate::i18n::text("Создать копию"),
        "catalogDescription" => crate::i18n::text(
            "Сохраняйте восстанавливаемую копию каталога и независимые читаемые метаданные.",
        ),
        "catalogExport" => crate::i18n::text("Экспорт метаданных"),
        "catalogExportDescription" => crate::i18n::text(
            "Переносимый JSON с именами, персонажами и их произведениями, исходными папками, относительными путями, избранным, порядком файлов и пресетами. Доступен другим инструментам без базы Nexa. Этот экспорт не предназначен для восстановления каталога.",
        ),
        "catalogExported" => crate::i18n::text("Сохранено: {path}"),
        "catalogNativeOnly" => crate::i18n::text(
            "Резервные копии, экспорт и восстановление доступны в настольном приложении.",
        ),
        "catalogOperationError" => crate::i18n::text("Ошибка операции с каталогом: {error}"),
        "catalogPreview" => crate::i18n::text("В копии файлов: {count}"),
        "catalogRestore" => crate::i18n::text("Восстановить каталог"),
        "catalogRestoreDescription" => crate::i18n::text(
            "Замена текущего каталога резервной копией Nexa. Перед заменой автоматически сохраняется страховочная копия. Оригиналы файлов не изменяются.",
        ),
        "catalogRestoreRequirements" => crate::i18n::text(
            "Сначала отключите и сохраните синхронизацию с диском и автоматическое сканирование. Укажите и сохраните исходные активные папки библиотеки и Stage из списка выше. Версия базы должна быть совместима. Восстанавливается весь каталог: более новая разметка будет заменена.",
        ),
        "catalogRestored" => crate::i18n::text("Каталог восстановлен. Страховочная копия: {path}"),
        "catalogSection" => crate::i18n::text("Копии"),
        "catalogWorking" => crate::i18n::text("Обработка каталога…"),
        "characterSeparator" => crate::i18n::text("Разделитель персонажей"),
        "checkDescription" => crate::i18n::text("Проверить наличие новой версии вручную."),
        "checkNow" => crate::i18n::text("Проверить сейчас"),
        "chooseDirectoryError" => crate::i18n::text("Не удалось выбрать папку: {error}"),
        "clearCache" => crate::i18n::text("Очистить кэш"),
        "clearDescription" => crate::i18n::text(
            "Удалить миниатюры из кэша. При необходимости они будут созданы заново.",
        ),
        "copyrightDescription" => crate::i18n::text(
            "Использовать первое по алфавиту произведение как папку. Сохранённые изменения реорганизуют распределённые файлы при включённой синхронизации.",
        ),
        "copyrightDirectory" => crate::i18n::text("Папка по произведению"),
        "downloadProgress" => crate::i18n::text("Прогресс загрузки обновления"),
        "gifDescription" => crate::i18n::text(
            "Подпапка для GIF при включённой синхронизации. Оставьте поле пустым, чтобы пропустить этот уровень. Сохранённые изменения реорганизуют распределённые GIF.",
        ),
        "gifFolder" => crate::i18n::text("Название подпапки для GIF"),
        "idle" => crate::i18n::text("Ожидание"),
        "importButton" => crate::i18n::text("Импортировать старую базу"),
        "importDescription" => crate::i18n::text(
            "Скопировать библиотеку, входящие и метаданные в новую базу. Старые данные и медиафайлы сохраняются. Существующие файлы пропускаются.",
        ),
        "importError" => crate::i18n::text("Импорт не выполнен: {error}"),
        "importNotice" => crate::i18n::text(
            "Персонажи сохраняют связь с произведениями; теги объединяются без дубликатов. Импортированные корневые папки сохраняются без изменения настроек. Входящие показывают файлы из сохранённой папки и очереди перераспределения. Библиотека показывает файлы из сохранённой папки библиотеки.",
        ),
        "importResult" => crate::i18n::text(
            "Импорт завершён. Добавлено в библиотеку: {libraryAdded}, во входящие: {stageAdded}. Пропущено существующих файлов: {skipped}.",
        ),
        "importTitle" => crate::i18n::text("Импорт из старой Nexa"),
        "importing" => crate::i18n::text("Импорт…"),
        "importingLegacy" => crate::i18n::text("Импорт данных старой Nexa…"),
        "indexing" => crate::i18n::text("Индексация"),
        "indexingDescription" => {
            crate::i18n::text("Сканируйте библиотеку и папку входящих файлов.")
        }
        "installNotice" => crate::i18n::text(
            "Nexa закроется и перезапустится во время установки. Сначала сохраните незавершённые изменения.",
        ),
        "installUpdate" => crate::i18n::text("Установить обновление"),
        "installVersion" => crate::i18n::text("Установить {version}"),
        "language" => crate::i18n::text("Язык"),
        "languageDescription" => {
            crate::i18n::text("Язык интерфейса. Применяется после сохранения изменений.")
        }
        "lastScan" => crate::i18n::text("Последнее сканирование"),
        "legacyCheckError" => crate::i18n::text("Не удалось проверить данные старой Nexa: {error}"),
        "library" => crate::i18n::text("Библиотека"),
        "libraryDescription" => {
            crate::i18n::text("Выберите папку для организованной медиабиблиотеки.")
        }
        "libraryPath" => crate::i18n::text("Папка библиотеки"),
        "loadSettingsError" => crate::i18n::text("Не удалось загрузить настройки: {error}"),
        "loadingSettings" => crate::i18n::text("Загрузка настроек…"),
        "logs" => crate::i18n::text("Журналы"),
        "logsDescription" => crate::i18n::text("Откройте журналы приложения."),
        "logsMissing" => crate::i18n::text("Папка журналов не настроена"),
        "notSelected" => crate::i18n::text("Не выбрано"),
        "openLogs" => crate::i18n::text("Открыть журналы"),
        "openLogsError" => crate::i18n::text("Не удалось открыть журналы: {error}"),
        "preview" => crate::i18n::text("Предпросмотр интерфейса"),
        "previewNotice" => crate::i18n::text(
            "Язык и тема применяются после сохранения и действуют до закрытия приложения.",
        ),
        "resetDefaults" => crate::i18n::text("Восстановить по умолчанию"),
        "saveChanges" => crate::i18n::text("Сохранить изменения"),
        "saveNotice" => crate::i18n::text(
            "Изменения применяются после сохранения. Сканирование использует сохранённые папки. Синхронизация с диском работает в фоне, если включена.",
        ),
        "saveSettingsError" => crate::i18n::text("Не удалось сохранить настройки: {error}"),
        "saved" => crate::i18n::text("Язык и тема применены в текущем сеансе."),
        "scanCounts" => {
            crate::i18n::text("Файлов: {scanned} · Обработано: {extracted} · Проблем: {errors}")
        }
        "scanLibrary" => crate::i18n::text("Сканировать библиотеку"),
        "scanStage" => crate::i18n::text("Сканировать входящие"),
        "scanning" => crate::i18n::text("Сканирование"),
        "sections" => crate::i18n::text("Разделы настроек"),
        "separatorDescription" => {
            crate::i18n::text("Разделитель имён персонажей в названиях файлов.")
        }
        "settings" => crate::i18n::text("Настройки"),
        "staging" => crate::i18n::text("Входящие"),
        "stagingDescription" => crate::i18n::text("Выберите папку для входящих медиафайлов."),
        "stagingPath" => crate::i18n::text("Папка входящих файлов"),
        "structure" => crate::i18n::text("Структура"),
        "structureDescription" => crate::i18n::text("Настройте организацию библиотеки на диске."),
        "sync" => crate::i18n::text("Синхронизировать структуру с диском"),
        "syncDescription" => crate::i18n::text(
            "При включении распределённые файлы постепенно перемещаются в заданную структуру. При выключении распределение сохраняет метаданные, оставляя файлы на месте; перемещения приостанавливаются после текущей операции.",
        ),
        "systemLanguage" => crate::i18n::text("Системный"),
        "theme" => crate::i18n::text("Тема"),
        "theme-arctic-blue" => crate::i18n::text("Арктический синий"),
        "theme-forest" => crate::i18n::text("Лес"),
        "theme-graphite" => crate::i18n::text("Графит"),
        "theme-paper" => crate::i18n::text("Бумага"),
        "theme-rosewood" => crate::i18n::text("Розовое дерево"),
        "theme-warm-amber" => crate::i18n::text("Тёплый янтарь"),
        "themeDescription" => {
            crate::i18n::text("Выберите цветовую тему. Применяется после сохранения.")
        }
        "themeName" => crate::i18n::text("Полночный фиолетовый"),
        "thumbnailCache" => crate::i18n::text("Кэш миниатюр"),
        "updates" => crate::i18n::text("Обновления"),
        "updatesDescription" => crate::i18n::text("Следите за текущей версией приложения."),
        "version" => crate::i18n::text("Текущая версия"),
        "videoFolder" => crate::i18n::text("Название подпапки для видео"),
        "videoFolderDescription" => crate::i18n::text("Название подпапки для видеофайлов."),
        _ => "",
    }
}
