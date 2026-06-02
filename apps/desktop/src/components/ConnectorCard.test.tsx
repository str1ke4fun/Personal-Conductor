import React from 'react';
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { ConnectorCard } from './ConnectorCard';

describe('ConnectorCard', () => {
  it('renders auth state, enabled state, and capability list', () => {
    render(
      <ConnectorCard
        connector={{
          id: 'lark',
          name: 'Lark',
          description: 'Lark workspace tools',
          implementation_type: 'native_rust',
          auth_status: 'authenticated',
          enabled: true,
          capabilities: [
            {
              capability: 'lark.doc',
              tools: ['lark.doc.search'],
              risk_level: 'low',
              requires_confirmation: false,
            },
            {
              capability: 'lark.doc.write',
              tools: ['lark.doc.create_or_update'],
              risk_level: 'medium',
              requires_confirmation: true,
            },
          ],
          config_json: null,
        }}
      />,
    );

    expect(screen.getByText('Lark')).toBeTruthy();
    expect(screen.getByText('已认证')).toBeTruthy();
    expect(screen.getByText('启用')).toBeTruthy();
    expect(screen.getByText('lark.doc')).toBeTruthy();
    expect(screen.getByText('lark.doc.write')).toBeTruthy();
    expect(screen.getByText('需确认')).toBeTruthy();
  });
});
