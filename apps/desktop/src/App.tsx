import { useEffect, useState } from 'react';
import { api } from './ipc/invoke';
import { Onboarding } from './components/Onboarding';
import { PetWindow } from './windows/PetWindow';

const ONBOARDING_DISMISSED_KEY = 'onboarding_dismissed';

export function App() {
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [checked, setChecked] = useState(false);

  useEffect(() => {
    // If user previously skipped onboarding, don't show again
    if (localStorage.getItem(ONBOARDING_DISMISSED_KEY)) {
      setChecked(true);
      return;
    }

    api
      .onboardingStatus()
      .then((status) => {
        if (!status.isComplete) {
          setShowOnboarding(true);
        }
      })
      .catch(() => {
        // On error, don't block the app
      })
      .finally(() => setChecked(true));
  }, []);

  function handleDismissOnboarding() {
    localStorage.setItem(ONBOARDING_DISMISSED_KEY, '1');
    setShowOnboarding(false);
  }

  return (
    <>
      <PetWindow />
      {checked && showOnboarding && <Onboarding onDismiss={handleDismissOnboarding} />}
    </>
  );
}
