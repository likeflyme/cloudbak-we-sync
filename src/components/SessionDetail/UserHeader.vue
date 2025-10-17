<template>
  <div class="user-header">
    <div class="user-avatar-section">
      <img
        class="user-main-avatar"
        :src="avatarSrc"
        @error="onAvatarError"
      />
      <div class="user-basic-info">
        <h2 class="user-title">{{ session.name }}</h2>
        <p class="user-subtitle">{{ session.desc }}</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';
import { endpoint } from '@/common/login';
import type { Session } from '@/models/session';

const props = defineProps<{
  session: Session;
}>();

const host = endpoint();

// Build avatar url from session
const buildAvatarUrl = () => `${host}/api/resources/relative-resource?relative_path=${props.session.wx_id}/head_image/${props.session.wx_id}.jpg&session_id=${props.session.id}`

// Mutable src so we can switch to fallback on error
const avatarSrc = ref<string>(buildAvatarUrl());

// Update avatar when session changes
watch(
  () => [props.session.wx_id, props.session.id],
  () => {
    avatarSrc.value = buildAvatarUrl();
  },
  { immediate: true }
);

const session = props.session;

const getDefaultAvatar = (name?: string) => {
  return `https://ui-avatars.com/api/?name=${encodeURIComponent(name || 'U')}&background=random&size=128`
}

const onAvatarError = () => {
  const fallback = getDefaultAvatar(props.session.name);
  if (avatarSrc.value !== fallback) {
    avatarSrc.value = fallback;
  }
}
</script>

<style scoped>
.user-header {
  /* Remove green gradient background for white theme */
  background: transparent;
  border-radius: 8px;
  padding: 24px;
  margin-bottom: 20px;
  /* Use neutral shadow on white background */
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.05);
  color: inherit;
}

.user-avatar-section {
  display: flex;
  align-items: center;
  gap: 20px;
}

.user-main-avatar {
  width: 80px;
  height: 80px;
  object-fit: cover;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.08);
  border: 2px solid #f0f0f0;
  border-radius: 6px;
}

.user-basic-info {
  flex: 1;
}

.user-title {
  font-size: 24px;
  font-weight: 500;
  margin: 0 0 6px 0;
  color: #333;
}

.user-subtitle {
  font-size: 14px;
  opacity: 0.9;
  margin: 0 0 12px 0;
  color: #666;
}

.user-badges {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}

/* 微信绿色主题色调整（保留） */
.n-tag--success-type {
  background: #07c160 !important;
  border-color: #07c160 !important;
}

/* 头像圆角样式 */
.n-avatar {
  border-radius: 6px !important;
}

.n-tag {
  border-radius: 12px;
}
</style>
