// Opt-in JCodec AAC smoke/bench driver for the Java runtime lane (TASK-19).
// Build: javac -cp jcodec-0.2.5.jar JCodecSmoke.java
// Run:   java  -cp jcodec-0.2.5.jar:. JCodecSmoke <adts|m4a|bench-adts> <file> [reps]
// Prints one JSON line per invocation; verify shape/checksum before any timing.

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.Locale;

import org.jcodec.codecs.aac.AACDecoder;
import org.jcodec.codecs.aac.AACUtils;
import org.jcodec.codecs.aac.ADTSParser;
import org.jcodec.common.io.NIOUtils;
import org.jcodec.common.model.AudioBuffer;
import org.jcodec.common.model.Packet;
import org.jcodec.containers.mp4.MP4TrackType;
import org.jcodec.containers.mp4.boxes.SampleEntry;
import org.jcodec.containers.mp4.demuxer.AbstractMP4DemuxerTrack;
import org.jcodec.containers.mp4.demuxer.MP4Demuxer;

public class JCodecSmoke {

    static class Sink {
        MessageDigest digest;
        long samples; // interleaved s16 values, not frames
        long frames;
        int rate;
        int channels;

        Sink() {
            try {
                digest = MessageDigest.getInstance("SHA-256");
            } catch (NoSuchAlgorithmException e) {
                throw new RuntimeException(e);
            }
        }

        void accept(AudioBuffer buf) {
            ByteBuffer data = buf.getData().duplicate();
            byte[] bytes = new byte[data.remaining()];
            data.get(bytes);
            digest.update(bytes);
            samples += bytes.length / 2;
            frames += 1;
            rate = buf.getFormat().getSampleRate();
            channels = buf.getFormat().getChannels();
        }

        String hex() {
            StringBuilder sb = new StringBuilder();
            for (byte b : digest.digest())
                sb.append(String.format("%02x", b));
            return sb.toString();
        }
    }

    static AACDecoder adtsDecoder(ByteBuffer firstFrame) throws IOException {
        // AACDecoder accepts an ADTS header (>= 7 bytes) as decoder-specific info.
        ByteBuffer head = firstFrame.duplicate();
        head.limit(Math.min(head.remaining(), 7));
        return new AACDecoder(head);
    }

    static void decodeAdts(String path, Sink sink, boolean timed, int reps) throws IOException {
        byte[] file = Files.readAllBytes(Paths.get(path));
        for (int rep = 0; rep < (timed ? reps : 1); rep++) {
            long t0 = System.nanoTime();
            ByteBuffer buf = ByteBuffer.wrap(file);
            AACDecoder dec = adtsDecoder(buf);
            while (buf.remaining() >= 7) {
                int frameStart = buf.position();
                ADTSParser.Header hdr = ADTSParser.read(buf);
                if (hdr == null)
                    break;
                buf.position(frameStart);
                ByteBuffer frame = buf.duplicate();
                frame.limit(frameStart + hdr.getSize());
                AudioBuffer out = dec.decodeFrame(frame, null);
                if (rep == 0 || timed) // shape recorded on every pass; digest only meaningful rep 0
                    sink.accept(out);
                buf.position(frameStart + hdr.getSize());
            }
            if (timed)
                System.out.println(String.format(Locale.US,
                        "{\"rep\":%d,\"wall_ms\":%.3f}", rep, (System.nanoTime() - t0) / 1e6));
        }
    }

    static void decodeM4a(String path, Sink sink) throws IOException {
        MP4Demuxer demux = MP4Demuxer.createMP4Demuxer(NIOUtils.readableFileChannel(path));
        // getAudioTracks() is List<DemuxerTrack>; the sample-entry API is on
        // the MP4 track. 0.2.5 does not let those two casts meet.
        AbstractMP4DemuxerTrack track = null;
        for (AbstractMP4DemuxerTrack cand : demux.getTracks()) {
            if (cand.getType() == MP4TrackType.SOUND) {
                track = cand;
                break;
            }
        }
        if (track == null)
            throw new IOException("no sound track");
        SampleEntry entry = track.getSampleEntries()[0];
        ByteBuffer asc = AACUtils.getCodecPrivate(entry);
        AACDecoder dec = new AACDecoder(asc);
        Packet pkt;
        while ((pkt = track.nextFrame()) != null) {
            ByteBuffer au = pkt.getData();
            // AACDecoder expects ADTS-wrapped frames internally: re-wrap the raw AU.
            ByteBuffer adts = ByteBuffer.allocate(au.remaining() + 7);
            ADTSParser.write(AACUtils.streamInfoToADTS(asc.duplicate(), true, 1, au.remaining()), adts);
            adts.position(7);
            adts.put(au);
            adts.flip();
            sink.accept(dec.decodeFrame(adts, null));
        }
    }

    public static void main(String[] args) throws Exception {
        if (args.length < 2) {
            System.err.println("usage: JCodecSmoke <adts|m4a|bench-adts> <file> [reps]");
            System.exit(2);
        }
        Sink sink = new Sink();
        String mode = args[0];
        if ("adts".equals(mode)) {
            decodeAdts(args[1], sink, false, 1);
        } else if ("m4a".equals(mode)) {
            decodeM4a(args[1], sink);
        } else if ("bench-adts".equals(mode)) {
            int reps = args.length > 2 ? Integer.parseInt(args[2]) : 20;
            decodeAdts(args[1], sink, true, reps); // rep timing lines precede the summary
        } else {
            System.err.println("unknown mode: " + mode);
            System.exit(2);
        }
        System.out.println(String.format(Locale.US,
                "{\"ok\":true,\"rate\":%d,\"channels\":%d,\"samples\":%d,\"frames\":%d,\"pcm_s16_sha256\":\"%s\"}",
                sink.rate, sink.channels, sink.samples, sink.frames, sink.hex()));
    }
}
