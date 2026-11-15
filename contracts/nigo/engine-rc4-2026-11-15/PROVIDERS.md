# 암호 Provider와 배포 조건

[모듈 개요](README.md)

이 문서는 고정된 외부 구현, 무결성 검증, native 실행 환경과 배포 전 확인 항목을 소유한다.
버전·checksum의 실행 기준은 [`build.gradle`](build.gradle)과 각 provider의 runtime preflight다.

`Bn254`는 EVM calldata나 gas를 알지 못하며 `BigInteger` point 값과 유효/무효 결과만 제공한다.
구현 provider는 Apache-2.0의 Hyperledger Besu
`org.hyperledger.besu.internal:besu-crypto-algorithms:25.12.0`에 고정한다. JAR SHA-256은
`1d2310308af1faf2412c9951a2ef84e31da4cf09bdad038bd6e29a3f15ac252f`이며 Gradle `check`가 실제
artifact를 검증한다. Provider artifact는 transitive dependency를 끄고 BN254 실행에 필요한 Tuweni Bytes와
Guava만 명시적으로 선택한다. Besu 타입은 공개 API에 노출하지 않는다.

`P256.verifyPrehashed`는 raw message hashing, EVM calldata와 gas를 소유하지 않는다. Provider는 Bouncy Castle
`org.bouncycastle:bcprov-jdk18on:1.78`과 Bouncy Castle Licence에 고정하고, Gradle `check`가 production에
연결된 JAR의 SHA-256 `1bf721b09758b3f55f2a5c875b6178ec6c41dddad854b0dead4b27a236f1943a`를
검증한다. EIP-7951의 160-byte codec, invalid-input empty-success와 6,900 gas는
[`nigo-vm`](../nigo-vm/README.md)이 소유한다.

`Kzg4844`는 Apache-2.0 `io.consensys.protocols:jc-kzg-4844:2.1.8`을 사용한다. JAR SHA-256
`5e532b112ee4f6f4dbbd0865466205a075c3fadd584fbee63624c1a4ab1d4e30`과 c-kzg v2.1.8 canonical
`trusted_setup.txt` 807,177 bytes의 SHA-256
`d39b9f2d047cc9dca2de58f264b6a09448ccd34db967881a6713eacacf0f26b7`을 build/runtime에서 검증한다.
검증한 setup bytes와 JNI가 로드하는 임시 파일은 동일하며 provider/setup은 process-wide 한 번만 초기화된다.
Invalid user proof만 `false`로 정규화하고 native/setup 내부 오류는 fail-fast한다. EIP-4844 192-byte frame,
versioned hash, Cancun activation, 50,000 gas와 exceptional failure는 `nigo-vm`이 소유한다.

jc-kzg JNI 지원 범위는 x86-64 Windows/glibc Linux/macOS와 aarch64 glibc Linux/macOS다. Alpine/musl Linux는
검증·지원 범위에 포함하지 않는다. Native library와 검증된 setup을 임시 파일로 staging하므로
`java.io.tmpdir`은 writable하고 native library load를 허용해야 한다. Unsupported
architecture, noexec temp 또는 setup/provider drift에는 Java fallback을 적용하지 않고 노드 시작을 실패시킨다.
Point-evaluation verify-only 경로에 full setup 전체가 수학적으로 필요하지는 않지만 KZG는 공식 c-kzg
binding과 canonical setup을 사용하는 선택의 운영 비용으로 수용한다. 별도 EIP-2537 BLS family는 아래의
gnark provider를 사용하며 KZG setup/provider lifecycle과 섞지 않는다.
jc-kzg 배포 JAR 자체에는 `LICENSE`/`NOTICE`가 포함되지 않으므로, 배포 가능한 fat JAR 또는 설치 이미지를
만들기 전 Apache-2.0 원문과 third-party attribution을 별도 패키징하는 release gate를 닫아야 한다.

`Bls12381`은 Apache-2.0 Hyperledger Besu `gnark:1.4.1`과 `besu-native-common:1.4.1`, JNA `5.16.0`을
사용한다. JAR SHA-256은 각각
`271f7a83c11cec285b937bf790768d72978be2b6b11b96e950ad0a1453f4d5ba`,
`d4c51b6c86a553ff355fc47e70da7661df77498efcd76894727f613a7b18187d`,
`3f5233589a799eb66dc2969afa3433fb56859d3d787c58b9bc7dd9e86f0a250c`이며 Gradle `check`가 production
artifact를 검증한다. 공개 API는 7개 operation과 valid/invalid 결과만 노출하고 Besu/JNA 타입을 외부로
내보내지 않는다. EIP-2537 frame, Prague activation, gas와 exceptional-failure 의미는 `nigo-vm`이 소유한다.

Provider JAR 안의 native resource도 runtime에서 별도 SHA-256을 확인한다.

| Resource | SHA-256 |
| --- | --- |
| `aarch64/libgnark_eip_2537.dylib` | `ef214ce5bb803d5fd31678d35592613c5201e22fea6572674bc76a6fb5d0e2f9` |
| `aarch64/libgnark_eip_2537.so` | `d853f255014cdcd70eac7291a8f97e55638dde59a3f43cbb33d1fa28d293dece` |
| `x86-64/libgnark_eip_2537.dylib` | `0d19145ecfab99213a783452b463b8003e1218309046f7f3bcf3436053d2a935` |
| `x86-64/libgnark_eip_2537.so` | `1852f00317017a0a4495228bbb42ec82d42480691d4853b545c58f06a710d1df` |

Class initialization은 현재 OS/architecture resource와 hash, native load를 preflight하고 MSM parallelism을
process-wide 1로 고정한다. 지원 범위는 glibc 2.34+ Linux와 macOS의 x86-64/aarch64다. JVM의 `os.arch`는
pinned Besu loader가 해석하는 `aarch64`, `x86-64` 및 명시된 x86 alias만 허용하며 `arm64` 같은 미지원
alias는 system-library fallback 전에 실패한다. Windows,
musl/Alpine와 riscv에는 fallback이 없으며 node 시작을 실패시킨다. Native extraction을 위해
`java.io.tmpdir`은 writable하고 executable해야 한다. 배포 전 gnark/common의 Apache-2.0과 JNA의
Apache-2.0/LGPL dual-license를 `THIRD_PARTY_NOTICES`와 SBOM에 포함하는 release gate가 남아 있다.

## 확인 명령

저장소 루트에서:

```bash
cd nigo-java
./gradlew :nigo-crypto:check
```

Native provider가 실제로 로드되는 지원 플랫폼별 검증과 license/NOTICE·SBOM 패키징은 별도
릴리스 검증 대상이다. 로컬 한 플랫폼의 성공을 모든 플랫폼 지원 근거로 해석하지 않는다.
