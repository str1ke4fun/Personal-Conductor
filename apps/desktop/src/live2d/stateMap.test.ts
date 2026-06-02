import { describe, it, expect } from 'vitest';
import { STATE_TO_EXPR, STATE_TO_MOTION, STATE_LABELS, type PetState } from './stateMap';

describe('stateMap', () => {
  const allStates: PetState[] = ['idle', 'working', 'update', 'quiet', 'new_task'];

  describe('STATE_TO_EXPR', () => {
    it('should have expressions for all states', () => {
      allStates.forEach((state) => {
        if (STATE_TO_EXPR[state]) {
          expect(typeof STATE_TO_EXPR[state]).toBe('string');
        }
      });
    });

    it('should have correct expression for idle state', () => {
      expect(STATE_TO_EXPR.idle).toBe('Idle');
    });

    it('should have correct expression for working state', () => {
      expect(STATE_TO_EXPR.working).toBe('Happy');
    });

    it('should have correct expression for update state', () => {
      expect(STATE_TO_EXPR.update).toBe('Happy');
    });

    it('should have correct expression for quiet state', () => {
      expect(STATE_TO_EXPR.quiet).toBe('Sleep');
    });

    it('should have correct expression for new_task state', () => {
      expect(STATE_TO_EXPR.new_task).toBe('Surprised');
    });
  });

  describe('STATE_TO_MOTION', () => {
    it('should have motion for all states', () => {
      allStates.forEach((state) => {
        expect(STATE_TO_MOTION[state]).toBeDefined();
        expect(STATE_TO_MOTION[state].group).toBeDefined();
        expect(typeof STATE_TO_MOTION[state].index).toBe('number');
      });
    });

    it('should have correct motion for idle state', () => {
      expect(STATE_TO_MOTION.idle).toEqual({ group: 'Idle', index: 0 });
    });

    it('should have correct motion for update state', () => {
      expect(STATE_TO_MOTION.update).toEqual({ group: 'Tap@Head', index: 0 });
    });
  });

  describe('STATE_LABELS', () => {
    it('should have labels for all states', () => {
      allStates.forEach((state) => {
        expect(STATE_LABELS[state]).toBeDefined();
        expect(typeof STATE_LABELS[state]).toBe('string');
      });
    });

    it('should have correct labels', () => {
      expect(STATE_LABELS.idle).toBe('正常运行');
      expect(STATE_LABELS.working).toBe('处理任务中');
      expect(STATE_LABELS.update).toBe('有新进展');
      expect(STATE_LABELS.quiet).toBe('专注模式');
      expect(STATE_LABELS.new_task).toBe('新任务到达');
    });
  });
});
