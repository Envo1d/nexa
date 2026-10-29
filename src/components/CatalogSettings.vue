<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

import { isNativeData } from "../runtime";
import SettingsRow from "./SettingsRow.vue";
import { translateUi as ui } from "../i18n/presentation";

interface BackupPreview {
  digest: string;
  createdAt: number;
  mediaCount: number;
  roots: string[];
}

const { t, locale } = useI18n();

const emit = defineEmits<{ working: [value: boolean]; restored: [] }>();

const native = isNativeData();

const busy = ref(false);
const message = ref("");
const error = ref("");
const selected = ref<{ path: string; preview: BackupPreview } | null>(null);
const confirmed = ref(false);

function working(value: boolean) {
  busy.value = value;
  emit("working", value);
}

async function exportCatalog(portable: boolean) {
  working(true);
  message.value = "";
  error.value = "";

  try {
    const stamp = new Date().toISOString().replace(/[:.]/g, "-");

    const path = await save({
      title: t(portable ? "catalogExport" : "catalogBackup"),
      defaultPath: `nexa-${portable ? "metadata" : "catalog"}-${stamp}.json`,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });

    if (!path) return;

    await invoke("export_catalog", { path, portable });

    message.value = t("catalogExported", { path });
  } catch (cause) {
    error.value = t("catalogOperationError", { error: ui(String(cause)) });
  } finally {
    working(false);
  }
}

async function selectBackup() {
  working(true);
  selected.value = null;
  confirmed.value = false;
  message.value = "";
  error.value = "";

  try {
    const path = await open({
      title: t("catalogRestore"),
      multiple: false,
      directory: false,
      filters: [{ name: "Nexa catalog JSON", extensions: ["json"] }],
    });

    if (typeof path !== "string") return;

    const preview = await invoke<BackupPreview>("inspect_catalog_backup", {
      path,
    });

    selected.value = { path, preview };
  } catch (cause) {
    error.value = t("catalogOperationError", { error: ui(String(cause)) });
  } finally {
    working(false);
  }
}

async function restoreCatalog() {
  if (!selected.value || !confirmed.value || busy.value) return;

  working(true);
  message.value = "";
  error.value = "";

  try {
    const safety = await invoke<string>("restore_catalog", {
      path: selected.value.path,
      digest: selected.value.preview.digest,
    });

    selected.value = null;
    confirmed.value = false;
    message.value = t("catalogRestored", { path: safety });
    emit("restored");
  } catch (cause) {
    error.value = t("catalogOperationError", { error: ui(String(cause)) });
  } finally {
    working(false);
  }
}
</script>

<template>
  <SettingsRow
    :title="t('catalogBackup')"
    :description="t('catalogBackupDescription')"
  >
    <button
      class="secondary-button"
      :disabled="!native || busy"
      @click="exportCatalog(false)"
    >
      {{ t("catalogCreateBackup") }}
    </button>
  </SettingsRow>
  <SettingsRow
    :title="t('catalogExport')"
    :description="t('catalogExportDescription')"
  >
    <button
      class="secondary-button"
      :disabled="!native || busy"
      @click="exportCatalog(true)"
    >
      {{ t("catalogExport") }}
    </button>
  </SettingsRow>
  <SettingsRow
    :title="t('catalogRestore')"
    :description="t('catalogRestoreDescription')"
  >
    <button
      class="secondary-button"
      :disabled="!native || busy"
      @click="selectBackup"
    >
      {{ t("catalogChooseBackup") }}
    </button>
  </SettingsRow>
  <div v-if="selected" class="catalog-restore-preview">
    <strong>{{
      t("catalogPreview", { count: selected.preview.mediaCount })
    }}</strong>
    <p>{{ new Date(selected.preview.createdAt).toLocaleString(locale) }}</p>
    <p class="catalog-path">{{ selected.path }}</p>
    <ul v-if="selected.preview.roots.length">
      <li
        v-for="root in selected.preview.roots"
        :key="root"
        class="catalog-path"
      >
        {{ root }}
      </li>
    </ul>
    <p>{{ t("catalogRestoreRequirements") }}</p>
    <label class="catalog-confirm">
      <input v-model="confirmed" type="checkbox" :disabled="busy" />
      <span>{{ t("catalogConfirmReplacement") }}</span>
    </label>
    <div class="catalog-actions">
      <button
        class="secondary-button"
        :disabled="busy"
        @click="selected = null"
      >
        {{ t("cancel") }}
      </button>
      <button
        class="primary-button"
        :disabled="busy || !confirmed"
        @click="restoreCatalog"
      >
        {{ t("catalogRestore") }}
      </button>
    </div>
  </div>
  <p v-if="busy || message" class="catalog-message" role="status">
    {{ busy ? t("catalogWorking") : message }}
  </p>
  <p v-if="error" class="catalog-message catalog-error" role="alert">
    {{ error }}
  </p>
  <p v-if="!native" class="catalog-message">{{ t("catalogNativeOnly") }}</p>
</template>

<style scoped>
.catalog-restore-preview {
  padding: 20px;
  border-top: 1px solid var(--line);
}

.catalog-restore-preview p,
.catalog-restore-preview ul {
  margin: 12px 0;
}

.catalog-path,
.catalog-message {
  overflow-wrap: anywhere;
}

.catalog-confirm {
  display: flex;
  align-items: flex-start;
  gap: 10px;
}

.catalog-confirm input {
  flex: 0 0 auto;
  margin-top: 4px;
}

.catalog-actions {
  display: flex;
  justify-content: flex-end;
  flex-wrap: wrap;
  gap: 12px;
  margin-top: 20px;
}

.catalog-message {
  margin: 0;
  padding: 16px 20px;
}

.catalog-error {
  color: var(--danger);
}
</style>
