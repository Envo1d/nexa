<script setup lang="ts">
import { computed, type Component } from "vue";
import {
  ArrowRight,
  ArrowUp,
  ArrowUpDown,
  Calendar,
  Database,
  FileText,
  FolderInput,
  GitBranch,
  Check,
  CircleCheck,
  CircleAlert,
  Download,
  LoaderCircle,
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
  Keyboard,
  ExternalLink,
  LayoutGrid,
  List,
  Menu,
  Minus,
  Play,
  Pause,
  Volume2,
  VolumeX,
  Maximize,
  Minimize,
  Plus,
  RotateCcw,
  Repeat,
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
  keyboard: Keyboard,
  external: ExternalLink,
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
  up: ArrowUp,
  check: Check,
  "check-circle": CircleCheck,
  warning: CircleAlert,
  download: Download,
  loader: LoaderCircle,
  info: Info,
  play: Play,
  pause: Pause,
  repeat: Repeat,
  volume: Volume2,
  "volume-off": VolumeX,
  fullscreen: Maximize,
  "fullscreen-exit": Minimize,
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
