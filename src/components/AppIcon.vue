<script setup lang="ts">
import { computed, type Component } from "vue";
import {
  ArrowRight,
  ArrowUpDown,
  Calendar,
  Database,
  FileText,
  FolderInput,
  GitBranch,
  Check,
  ChevronDown,
  Clock,
  Copy,
  Ellipsis,
  Files,
  Film,
  Folder,
  Heart,
  Image,
  Info,
  LayoutGrid,
  List,
  Menu,
  Minus,
  Play,
  Plus,
  RotateCcw,
  Scan,
  Search,
  Settings,
  Share2,
  Square,
  Star,
  Tag,
  Trash2,
  Video,
  X,
} from "@lucide/vue";

const props = withDefaults(defineProps<{ name: string; size?: number }>(), {
  size: 20,
});

// Explicit imports preserve the semantic API without bundling the full icon catalogue.
const icons: Record<string, Component> = {
  menu: Menu,
  gallery: Folder,
  heart: Heart,
  star: Star,
  settings: Settings,
  grid: LayoutGrid,
  list: List,
  search: Search,
  image: Image,
  video: Video,
  files: Files,
  down: ChevronDown,
  sort: ArrowUpDown,
  reset: RotateCcw,
  close: X,
  minimize: Minus,
  maximize: Square,
  restore: Copy,
  more: Ellipsis,
  plus: Plus,
  clock: Clock,
  size: Scan,
  calendar: Calendar,
  tag: Tag,
  trash: Trash2,
  share: Share2,
  copy: Copy,
  arrow: ArrowRight,
  check: Check,
  info: Info,
  play: Play,
  film: Film,
  cache: Database,
  logs: FileText,
  staging: FolderInput,
  structure: GitBranch,
};
const icon = computed(() => icons[props.name] ?? Info);
</script>

<template>
  <component
    :is="icon"
    :size="size"
    :stroke-width="2"
    aria-hidden="true"
    focusable="false"
  />
</template>
