package app.coomi;

import android.content.Intent;
import android.os.Bundle;
import android.view.LayoutInflater;
import android.view.View;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.TextView;

import androidx.annotation.NonNull;
import androidx.annotation.Nullable;
import androidx.annotation.StringRes;
import androidx.core.content.ContextCompat;
import androidx.fragment.app.Fragment;

import com.termux.R;

/** Step 1: Open the shared Provider management page. */
public class AuthFragment extends Fragment implements CoomiSetupActivity.StepFragment {

    private TextView mStatusText;

    @Nullable
    @Override
    public View onCreateView(
        @NonNull LayoutInflater inflater,
        @Nullable ViewGroup container,
        @Nullable Bundle savedInstanceState
    ) {
        View view = inflater.inflate(R.layout.fragment_coomi_auth, container, false);
        mStatusText = view.findViewById(R.id.auth_status);
        Button providersButton = view.findViewById(R.id.btn_open_providers);
        providersButton.setOnClickListener(ignored -> openProviders());
        updateStatus();
        return view;
    }

    @Override
    public void onResume() {
        super.onResume();
        updateStatus();
    }

    private void openProviders() {
        Intent intent = new Intent(requireContext(), com.termux.app.CoomiActivity.class);
        intent.putExtra(com.termux.app.CoomiActivity.EXTRA_ROUTE, "#/providers");
        intent.putExtra(com.termux.app.CoomiActivity.EXTRA_RETURN_TO_SETUP, true);
        startActivity(intent);
    }

    private void updateStatus() {
        if (mStatusText == null) return;
        if (CoomiDemo.isEnabled() || CoomiConfig.isConfigured()) {
            setStatus(R.string.coomi_auth_provider_ready, R.color.coomi_ok);
        } else {
            setStatus(R.string.coomi_auth_provider_required, R.color.coomi_text_2);
        }
    }

    private void setStatus(@StringRes int textRes, int colorRes) {
        mStatusText.setTextColor(ContextCompat.getColor(mStatusText.getContext(),
            CoomiTheme.isDark(requireActivity()) ? nightColor(colorRes) : colorRes));
        mStatusText.setText(textRes);
    }

    private int nightColor(int lightRes) {
        if (lightRes == R.color.coomi_ok) return R.color.coomi_night_ok;
        if (lightRes == R.color.coomi_text_2) return R.color.coomi_night_text_2;
        return lightRes;
    }

    @Override
    public boolean handleNext() {
        if (CoomiDemo.isEnabled() || CoomiConfig.isConfigured()) return false;
        setStatus(R.string.coomi_auth_provider_required, R.color.coomi_danger);
        return true;
    }
}
