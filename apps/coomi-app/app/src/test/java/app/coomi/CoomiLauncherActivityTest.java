package app.coomi;

import org.junit.Test;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class CoomiLauncherActivityTest {

    @Test
    public void refreshesRootStatusAfterRequestInCurrentSession() {
        assertTrue(CoomiLauncherActivity.shouldRefreshRootStatus(false, true));
    }

    @Test
    public void refreshesRootStatusWhenPreviousCheckWasGranted() {
        assertTrue(CoomiLauncherActivity.shouldRefreshRootStatus(true, false));
    }

    @Test
    public void skipsRootStatusRefreshBeforeUserRequestsIt() {
        assertFalse(CoomiLauncherActivity.shouldRefreshRootStatus(false, false));
    }
}
