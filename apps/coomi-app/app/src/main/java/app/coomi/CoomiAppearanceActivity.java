package app.coomi;

import android.app.Activity;
import android.content.Intent;
import android.os.Bundle;
import android.widget.RadioButton;

import com.termux.R;

/** Dedicated appearance page. Theme changes are broadcast and applied immediately. */
public class CoomiAppearanceActivity extends Activity {
    private final int[] rowIds = {
        R.id.btn_appearance_system, R.id.btn_appearance_light, R.id.btn_appearance_dark,
        R.id.btn_appearance_book, R.id.btn_appearance_orange
    };
    private final int[] radioIds = {
        R.id.radio_appearance_system, R.id.radio_appearance_light, R.id.radio_appearance_dark,
        R.id.radio_appearance_book, R.id.radio_appearance_orange
    };
    private final String[] modes = {"system", "light", "dark", "book", "orange"};

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        CoomiTheme.applyPageTheme(this);
        super.onCreate(savedInstanceState);
        setContentView(R.layout.activity_coomi_appearance);
        CoomiTheme.applyPageSystemBars(this);
        findViewById(R.id.btn_appearance_back).setOnClickListener(v -> finish());
        for (int index = 0; index < modes.length; index++) {
            final String mode = modes[index];
            findViewById(rowIds[index]).setOnClickListener(v -> select(mode));
        }
        refreshChecks();
    }

    private void select(String mode) {
        if (mode.equals(CoomiTheme.getMode(this))) return;
        CoomiTheme.setMode(this, mode);
        sendBroadcast(new Intent(CoomiTheme.ACTION_THEME_CHANGED).setPackage(getPackageName()));
        recreate();
    }

    private void refreshChecks() {
        String selected = CoomiTheme.getMode(this);
        for (int index = 0; index < modes.length; index++) {
            ((RadioButton) findViewById(radioIds[index])).setChecked(modes[index].equals(selected));
        }
    }
}
