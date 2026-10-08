// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import { Select } from '@/components/ui/select';
import type { AnalysisResult } from '@/lib/models/analysis';
import { ByteSizeService } from '@/lib/services/byte-size-service';

import MdAnalysisVisualPane from './md-analysis-visual-pane.vue';
import MdAnalysisSunburst from './md-analysis-sunburst.vue';

const result: AnalysisResult = {
  scanId: 7,
  root: '/fixture',
  scannedAtMs: 1_000,
  totalBytes: 64,
  skippedCount: 0,
  truncated: false,
  entries: [],
};

function mountPane(exclusionsActive: boolean) {
  return mount(MdAnalysisVisualPane, {
    props: {
      result,
      entries: [],
      folderCount: 0,
      viewMode: 'treemap',
      exclusionsActive,
      openDisabled: false,
      deleteDisabled: false,
    },
    global: {
      plugins: [i18n],
      stubs: {
        MdAnalysisTreemap: true,
        MdAnalysisSunburst: true,
        MdIcon: true,
      },
    },
  });
}

describe('analysis result exclusions', () => {
  beforeEach(() => {
    vi.spyOn(ByteSizeService, 'bytes').mockReturnValue('64 B');
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('keeps the result toolbar clear when analysis includes all folders', () => {
    expect(mountPane(false).find('.exclusion-link').exists()).toBe(false);
  });

  it('explains a filtered result without changing the reported total', () => {
    const wrapper = mountPane(true);

    expect(wrapper.get('.exclusion-link').text()).toBe('Exclusions on');
    expect(wrapper.get('.space-summary').text()).toContain('64 B');
  });

  it('opens the shared exclusion settings from the filtered result hint', async () => {
    const wrapper = mountPane(true);

    expect(wrapper.get('.exclusion-link').element.tagName).toBe('BUTTON');
    await wrapper.get('.exclusion-link').trigger('click');

    expect(wrapper.emitted('openExclusions')).toHaveLength(1);
  });

  it('forwards sunburst context actions and operation guards to the page', async () => {
    const wrapper = mountPane(false);
    try {
      await wrapper.setProps({ viewMode: 'sunburst', deleteDisabled: true, deletingPath: '/fixture/A' });
      const chart = wrapper.getComponent(MdAnalysisSunburst);
      expect(chart.props('deleteDisabled')).toBe(true);
      expect(chart.props('deletingPath')).toBe('/fixture/A');
      const entry = {
        name: 'A',
        path: '/fixture/A',
        bytes: 64,
        fileCount: 1,
        isDirectory: true,
        modifiedAtMs: null,
        contentFingerprint: null,
      };
      chart.vm.$emit('openEntry', entry);
      chart.vm.$emit('reveal', entry.path);
      chart.vm.$emit('delete', entry);
      expect(wrapper.emitted('openEntry')).toEqual([[entry]]);
      expect(wrapper.emitted('reveal')).toEqual([[entry.path]]);
      expect(wrapper.emitted('delete')).toEqual([[entry]]);
    } finally {
      wrapper.unmount();
    }
  });

  it('places depth beside the chart switcher and preserves independent levels without a footer', async () => {
    const wrapper = mountPane(false);
    try {
      expect(wrapper.find('header .chart-controls .chart-depth').exists()).toBe(true);
      expect(wrapper.find('footer').exists()).toBe(false);
      wrapper.getComponent(Select).vm.$emit('update:modelValue', '6');
      await wrapper.vm.$nextTick();
      await wrapper.setProps({ viewMode: 'sunburst' });
      expect(wrapper.getComponent(Select).props('modelValue')).toBe('3');
      wrapper.getComponent(Select).vm.$emit('update:modelValue', '1');
      await wrapper.vm.$nextTick();
      expect(wrapper.getComponent(Select).props('modelValue')).toBe('3');
      wrapper.getComponent(Select).vm.$emit('update:modelValue', '4');
      await wrapper.vm.$nextTick();
      await wrapper.setProps({ viewMode: 'treemap', openDisabled: true });
      expect(wrapper.getComponent(Select).props('modelValue')).toBe('6');
      expect(wrapper.getComponent(Select).props('disabled')).toBe(true);
      await wrapper.setProps({ viewMode: 'sunburst' });
      expect(wrapper.getComponent(Select).props('modelValue')).toBe('4');
    } finally {
      wrapper.unmount();
    }
  });
});
