// @vitest-environment happy-dom

import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import { ClipboardService } from '@/lib/services/clipboard-service';
import { FeedbackService } from '@/lib/services/feedback-service';
import { NativeDragDropService } from '@/lib/services/native-drag-drop-service';

import MdFeedbackDialog from './md-feedback-dialog.vue';
import feedbackDialogSource from './md-feedback-dialog.vue?raw';

const passthroughStub = { template: '<div><slot /></div>' };
const dialogContentStub = {
  props: {
    height: {
      type: String,
      default: 'auto',
    },
    size: String,
  },
  template:
    '<section class="dialog-content-stub" :data-dialog-height="height" :data-dialog-size="size"><slot /></section>',
};
const buttonStub = {
  inheritAttrs: false,
  props: { disabled: Boolean },
  emits: ['click'],
  template: '<button :class="$attrs.class" :disabled="disabled" @click="$emit(\'click\', $event)"><slot /></button>',
};
const iconActionStub = {
  inheritAttrs: false,
  props: { disabled: Boolean, label: String },
  emits: ['click'],
  template: '<button :aria-label="label" :disabled="disabled" @click="$emit(\'click\', $event)"><slot /></button>',
};

function mountDialog() {
  return mount(MdFeedbackDialog, {
    props: { open: true },
    global: {
      plugins: [i18n],
      stubs: {
        Button: buttonStub,
        Checkbox: passthroughStub,
        Dialog: passthroughStub,
        DialogDescription: passthroughStub,
        DialogTitle: passthroughStub,
        Input: passthroughStub,
        MdDialogContent: dialogContentStub,
        MdDialogFooter: passthroughStub,
        MdIcon: true,
        MdIconAction: iconActionStub,
        MdSpinner: true,
        Select: passthroughStub,
        SelectContent: passthroughStub,
        SelectItem: passthroughStub,
        SelectTrigger: passthroughStub,
        SelectValue: passthroughStub,
      },
    },
  });
}

function textFile(name: string): File {
  return new File([`attachment:${name}`], name, { type: 'text/plain' });
}

async function selectFiles(wrapper: ReturnType<typeof mountDialog>, files: File[]) {
  const input = wrapper.get('input[type="file"]');
  Object.defineProperty(input.element, 'files', { configurable: true, value: files });
  await input.trigger('change');
  await flushPromises();
}

describe('feedback dialog component', () => {
  beforeEach(() => {
    vi.spyOn(NativeDragDropService, 'listen').mockResolvedValue(() => undefined);
    vi.spyOn(FeedbackService, 'stageAttachment').mockImplementation(async file => ({
      token: `token-${file.name}`,
      displayName: file.name,
      mimeType: file.type,
      size: file.size,
    }));
    vi.spyOn(FeedbackService, 'discardAttachments').mockResolvedValue();
    vi.spyOn(FeedbackService, 'submit').mockResolvedValue({
      id: 'feedback-1',
      createdAt: '2026-08-31T00:00:00Z',
      submittedLogCount: 0,
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('uses legacy WebKit-safe semantic surfaces for attachment interactions', () => {
    expect(feedbackDialogSource).toContain('.attachment-dropzone-action:hover');
    expect(feedbackDialogSource).toContain('background: var(--surface-primary-subtle)');
    expect(feedbackDialogSource).toContain('background: var(--surface-destructive-subtle)');
    expect(feedbackDialogSource).not.toContain('hover:bg-primary/10');
    expect(feedbackDialogSource).not.toContain('hover:bg-destructive/10');
    expect(feedbackDialogSource).not.toContain('color-mix(in');
  });

  it('keeps the feedback form content-sized instead of reserving a tall empty footer area', () => {
    const wrapper = mountDialog();

    expect(wrapper.get('.dialog-content-stub').attributes('data-dialog-height')).toBe('auto');
    expect(wrapper.get('.dialog-content-stub').attributes('data-dialog-size')).toBe('wide');
    expect(wrapper.get('.feedback-body').classes()).toContain('scrollbar-stable');
    wrapper.unmount();
  });

  it('stages five attachments through the real file input and prevents a sixth attachment', async () => {
    const wrapper = mountDialog();
    const files = Array.from({ length: 5 }, (_, index) => textFile(`attachment-${index + 1}.txt`));

    await selectFiles(wrapper, files);

    expect(wrapper.findAll('.attachment-list li')).toHaveLength(5);
    expect(FeedbackService.stageAttachment).toHaveBeenCalledTimes(5);
    expect(wrapper.get('.attachment-picker-button').attributes('disabled')).toBeDefined();

    await selectFiles(wrapper, [textFile('attachment-6.txt')]);

    expect(FeedbackService.stageAttachment).toHaveBeenCalledTimes(5);
    expect(wrapper.findAll('.attachment-list li')).toHaveLength(5);
    wrapper.unmount();
  });

  it('discards staged attachment tokens when the user closes the dialog', async () => {
    const wrapper = mountDialog();
    await selectFiles(wrapper, [textFile('diagnostic.txt')]);

    await wrapper.get('.feedback-primary-actions button:first-child').trigger('click');
    await flushPromises();

    expect(wrapper.emitted('update:open')).toEqual([[false]]);
    expect(FeedbackService.discardAttachments).toHaveBeenCalledWith(['token-diagnostic.txt']);
    wrapper.unmount();
  });

  it('submits the visible form state and renders the terminal success state', async () => {
    const wrapper = mountDialog();
    await selectFiles(wrapper, [textFile('context.txt')]);
    await wrapper.get('textarea').setValue('The cleanup dialog failed after selecting a directory.');

    await wrapper.get('.feedback-primary-actions button:last-child').trigger('click');
    await flushPromises();

    expect(FeedbackService.submit).toHaveBeenCalledWith(
      expect.objectContaining({
        category: 'issue',
        content: 'The cleanup dialog failed after selecting a directory.',
        includeLogs: true,
        attachmentTokens: ['token-context.txt'],
      })
    );
    expect(wrapper.find('.feedback-success').exists()).toBe(true);
    expect(wrapper.text()).toContain('feedback-1');
    const header = wrapper.get('.feedback-success-heading');
    expect(header.text()).toBe(i18n.global.t('settings.feedbackDialog.successTitle'));
    expect(header.find('[data-tauri-drag-region][aria-hidden="true"]').exists()).toBe(true);
    expect(header.find('.feedback-success-icon').exists()).toBe(false);
    expect(header.find('.feedback-reference').exists()).toBe(false);
    expect(wrapper.get('.feedback-success-description').text()).toBe(
      i18n.global.t('settings.feedbackDialog.successDescription')
    );
    expect(wrapper.get('.feedback-success').text()).not.toContain(
      i18n.global.t('settings.feedbackDialog.successTitle')
    );
    expect(wrapper.get('.dialog-content-stub').attributes('data-dialog-size')).toBe('standard');
    vi.spyOn(ClipboardService, 'writeText').mockResolvedValue();
    await wrapper.get('.feedback-reference button').trigger('click');
    await flushPromises();
    expect(ClipboardService.writeText).toHaveBeenCalledWith('feedback-1');
    wrapper.unmount();
  });
});
