<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";

import { translateUi as ui } from "../i18n/presentation";
import { useModalFocus } from "../shared/composables/useModalFocus";
import AppIcon from "./AppIcon.vue";

defineProps<{ version: string }>();

const emit = defineEmits<{ answer: [accepted: boolean] }>();

const dialog = ref<HTMLElement | null>(null);

const cancelButton = ref<HTMLButtonElement | null>(null);

const open = ref(true);

const trigger =
  document.activeElement instanceof HTMLElement ? document.activeElement : null;

const { trapFocus } = useModalFocus(open, cancelButton, dialog);

function keyboard(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    emit("answer", false);

    return;
  }

  trapFocus(event);
}

onMounted(() => cancelButton.value?.focus());

onBeforeUnmount(() => {
  if (trigger?.isConnected) trigger.focus({ preventScroll: true });
});
</script>

<template>
  <Teleport to="body">
    <div class="modal-backdrop" @click.self="emit('answer', false)">
      <section
        ref="dialog"
        class="modal update-confirm-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="update-confirm-title"
        aria-describedby="update-confirm-description update-confirm-warning"
        @keydown.stop="keyboard"
      >
        <header class="panel-heading">
          <div class="update-confirm-heading">
            <span class="update-confirm-icon">
              <AppIcon name="reset" :size="22" />
            </span>
            <h2 id="update-confirm-title">{{ ui("Install update?") }}</h2>
          </div>
          <button
            type="button"
            class="icon-button"
            :aria-label="ui('Cancel update')"
            @click="emit('answer', false)"
          >
            <AppIcon name="close" />
          </button>
        </header>

        <div class="update-confirm-release">
          <img src="/icon.png" alt="" width="38" height="38" />
          <div>
            <span>{{ ui("NEW VERSION") }}</span>
            <strong>Nexa {{ version }}</strong>
          </div>
        </div>

        <p id="update-confirm-description" class="update-confirm-description">
          {{
            ui(
              "Nexa will download and verify the update, then close to install it. The app will reopen automatically when the update is complete.",
            )
          }}
        </p>
        <div id="update-confirm-warning" class="update-confirm-warning">
          <AppIcon name="info" :size="18" />
          <p>{{ ui("Save any unfinished edits before continuing.") }}</p>
        </div>

        <footer class="modal-footer">
          <button
            ref="cancelButton"
            type="button"
            class="secondary-button"
            @click="emit('answer', false)"
          >
            {{ ui("Cancel") }}
          </button>
          <button
            type="button"
            class="primary-button"
            @click="emit('answer', true)"
          >
            <AppIcon name="check" :size="15" />{{ ui("Install update") }}
          </button>
        </footer>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
.update-confirm-dialog {
  width: min(460px, calc(100vw - 40px));
}

.update-confirm-heading {
  display: flex;
  align-items: center;
  gap: 12px;
}

.update-confirm-icon {
  display: grid;
  place-items: center;
  width: 40px;
  height: 40px;
  border: 1px solid #69448d;
  border-radius: 10px;
  background: #a786ff15;
  color: #c5a5f7;
}

.update-confirm-release {
  display: flex;
  align-items: center;
  gap: 14px;
  margin: 12px 0 18px;
  padding: 16px;
  border: 1px solid #483359;
  border-radius: 9px;
  background: #a786ff08;
}

.update-confirm-release div {
  display: grid;
  gap: 5px;
  min-width: 0;
}

.update-confirm-release span {
  color: #a89bb9;
  font-size: 9px;
  letter-spacing: 1.5px;
}

.update-confirm-release strong {
  font-size: 16px;
  font-weight: 600;
  overflow-wrap: anywhere;
}

.update-confirm-description {
  color: #b8abc8;
  font-size: 12px;
  line-height: 1.7;
}

.update-confirm-warning {
  display: flex;
  align-items: flex-start;
  gap: 9px;
  margin-top: 16px;
  padding: 12px;
  border-radius: 7px;
  background: #a786ff10;
  color: #c5b2dc;
  font-size: 11px;
  line-height: 1.6;
}
</style>
