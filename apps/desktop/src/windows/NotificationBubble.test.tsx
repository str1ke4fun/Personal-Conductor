import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { NotificationBubble } from './NotificationBubble';

describe('NotificationBubble', () => {
  const mockNotification = {
    id: 'test-id',
    content: 'Test notification',
    urgency: 'low' as const,
  };

  const mockOnClose = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    cleanup();
  });

  it('should render notification content', () => {
    render(<NotificationBubble notification={mockNotification} onClose={mockOnClose} />);
    
    // Wait for animation to complete
    vi.advanceTimersByTime(100);
    
    expect(screen.getByText('Test notification')).toBeTruthy();
  });

  it('should show correct icon based on urgency', () => {
    const notifications = [
      { ...mockNotification, urgency: 'low' as const },
      { ...mockNotification, urgency: 'medium' as const },
      { ...mockNotification, urgency: 'high' as const },
    ];

    notifications.forEach((notification) => {
      const { container, unmount } = render(<NotificationBubble notification={notification} onClose={mockOnClose} />);
      vi.advanceTimersByTime(100);
      
      const icon = container.querySelector('.notification-icon');
      if (notification.urgency === 'high') {
        expect(icon?.textContent).toBe('⚠️');
      } else if (notification.urgency === 'medium') {
        expect(icon?.textContent).toBe('📢');
      } else {
        expect(icon?.textContent).toBe('💬');
      }
      
      unmount();
    });
  });

  it('should call onClose after 10 seconds', () => {
    render(<NotificationBubble notification={mockNotification} onClose={mockOnClose} />);
    
    expect(mockOnClose).not.toHaveBeenCalled();
    
    // Wait for 10 seconds
    vi.advanceTimersByTime(10000);
    
    // Animation takes 300ms to complete
    vi.advanceTimersByTime(300);
    
    expect(mockOnClose).toHaveBeenCalledWith('test-id');
  });

  it('should call onClose when clicked', () => {
    render(<NotificationBubble notification={mockNotification} onClose={mockOnClose} />);
    vi.advanceTimersByTime(100);
    
    const bubble = screen.getByText('Test notification').parentElement;
    bubble?.click();
    
    // Animation takes 300ms to complete
    vi.advanceTimersByTime(300);
    
    expect(mockOnClose).toHaveBeenCalledWith('test-id');
  });
});
