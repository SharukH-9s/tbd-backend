--
-- PostgreSQL database dump
--

\restrict qnNR1PpCqIoLa8KpmeIjEEMxLg2OHCBVVA7QlFRJsFQfoCavYCstWgtOgU5lqsX

-- Dumped from database version 18.3
-- Dumped by pg_dump version 18.3

-- Started on 2026-05-07 16:31:34

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- TOC entry 231 (class 1259 OID 25058)
-- Name: bookings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.bookings (
    id integer NOT NULL,
    slot_id integer NOT NULL,
    user_id integer,
    contact_name text,
    contact_phone text,
    payment_status text DEFAULT 'Unpaid'::text NOT NULL,
    booking_time timestamp with time zone DEFAULT now() NOT NULL
);


--
-- TOC entry 230 (class 1259 OID 25057)
-- Name: bookings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.bookings_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- TOC entry 4985 (class 0 OID 0)
-- Dependencies: 230
-- Name: bookings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.bookings_id_seq OWNED BY public.bookings.id;


--
-- TOC entry 229 (class 1259 OID 24970)
-- Name: game_settings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.game_settings (
    id integer NOT NULL,
    game_id integer NOT NULL,
    label text NOT NULL,
    base_price numeric(10,2) NOT NULL,
    slot_duration_mins integer NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- TOC entry 228 (class 1259 OID 24969)
-- Name: game_settings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.game_settings_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- TOC entry 4986 (class 0 OID 0)
-- Dependencies: 228
-- Name: game_settings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.game_settings_id_seq OWNED BY public.game_settings.id;


--
-- TOC entry 225 (class 1259 OID 24925)
-- Name: games; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.games (
    id integer NOT NULL,
    turf_id integer NOT NULL,
    name text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    game_start_time time without time zone DEFAULT '08:00:00'::time without time zone NOT NULL,
    game_end_time time without time zone DEFAULT '01:00:00'::time without time zone NOT NULL
);


--
-- TOC entry 224 (class 1259 OID 24924)
-- Name: games_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.games_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- TOC entry 4987 (class 0 OID 0)
-- Dependencies: 224
-- Name: games_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.games_id_seq OWNED BY public.games.id;


--
-- TOC entry 221 (class 1259 OID 16588)
-- Name: refresh_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.refresh_tokens (
    token text NOT NULL,
    user_id integer NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- TOC entry 227 (class 1259 OID 24944)
-- Name: slots; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.slots (
    id integer NOT NULL,
    game_id integer NOT NULL,
    start_time timestamp with time zone NOT NULL,
    end_time timestamp with time zone NOT NULL,
    status text DEFAULT 'Available'::text NOT NULL,
    pending_until timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    settings_id integer,
    final_price numeric(10,2) NOT NULL,
    label text
);


--
-- TOC entry 226 (class 1259 OID 24943)
-- Name: slots_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.slots_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- TOC entry 4988 (class 0 OID 0)
-- Dependencies: 226
-- Name: slots_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.slots_id_seq OWNED BY public.slots.id;


--
-- TOC entry 223 (class 1259 OID 16627)
-- Name: turfs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.turfs (
    id integer NOT NULL,
    manager_id integer NOT NULL,
    name text NOT NULL,
    location text NOT NULL,
    description text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- TOC entry 222 (class 1259 OID 16626)
-- Name: turfs_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.turfs_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- TOC entry 4989 (class 0 OID 0)
-- Dependencies: 222
-- Name: turfs_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.turfs_id_seq OWNED BY public.turfs.id;


--
-- TOC entry 220 (class 1259 OID 16565)
-- Name: users; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.users (
    id integer NOT NULL,
    username text NOT NULL,
    email text NOT NULL,
    password_hash text NOT NULL,
    phone text NOT NULL,
    role text DEFAULT 'User'::text NOT NULL,
    location text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- TOC entry 219 (class 1259 OID 16564)
-- Name: users_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.users_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- TOC entry 4990 (class 0 OID 0)
-- Dependencies: 219
-- Name: users_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.users_id_seq OWNED BY public.users.id;


--
-- TOC entry 4801 (class 2604 OID 25061)
-- Name: bookings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bookings ALTER COLUMN id SET DEFAULT nextval('public.bookings_id_seq'::regclass);


--
-- TOC entry 4799 (class 2604 OID 24973)
-- Name: game_settings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.game_settings ALTER COLUMN id SET DEFAULT nextval('public.game_settings_id_seq'::regclass);


--
-- TOC entry 4792 (class 2604 OID 24928)
-- Name: games id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.games ALTER COLUMN id SET DEFAULT nextval('public.games_id_seq'::regclass);


--
-- TOC entry 4796 (class 2604 OID 24947)
-- Name: slots id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.slots ALTER COLUMN id SET DEFAULT nextval('public.slots_id_seq'::regclass);


--
-- TOC entry 4789 (class 2604 OID 16630)
-- Name: turfs id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.turfs ALTER COLUMN id SET DEFAULT nextval('public.turfs_id_seq'::regclass);


--
-- TOC entry 4784 (class 2604 OID 16568)
-- Name: users id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users ALTER COLUMN id SET DEFAULT nextval('public.users_id_seq'::regclass);


--
-- TOC entry 4823 (class 2606 OID 25071)
-- Name: bookings bookings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bookings
    ADD CONSTRAINT bookings_pkey PRIMARY KEY (id);


--
-- TOC entry 4821 (class 2606 OID 24985)
-- Name: game_settings game_settings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.game_settings
    ADD CONSTRAINT game_settings_pkey PRIMARY KEY (id);


--
-- TOC entry 4815 (class 2606 OID 24937)
-- Name: games games_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.games
    ADD CONSTRAINT games_pkey PRIMARY KEY (id);


--
-- TOC entry 4811 (class 2606 OID 16598)
-- Name: refresh_tokens refresh_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.refresh_tokens
    ADD CONSTRAINT refresh_tokens_pkey PRIMARY KEY (token);


--
-- TOC entry 4819 (class 2606 OID 24959)
-- Name: slots slots_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.slots
    ADD CONSTRAINT slots_pkey PRIMARY KEY (id);


--
-- TOC entry 4813 (class 2606 OID 16643)
-- Name: turfs turfs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.turfs
    ADD CONSTRAINT turfs_pkey PRIMARY KEY (id);


--
-- TOC entry 4805 (class 2606 OID 16587)
-- Name: users users_email_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_email_key UNIQUE (email);


--
-- TOC entry 4807 (class 2606 OID 16583)
-- Name: users users_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);


--
-- TOC entry 4809 (class 2606 OID 16585)
-- Name: users users_username_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_username_key UNIQUE (username);


--
-- TOC entry 4824 (class 1259 OID 25082)
-- Name: idx_bookings_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_bookings_user ON public.bookings USING btree (user_id);


--
-- TOC entry 4816 (class 1259 OID 24998)
-- Name: idx_slots_game_time; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_slots_game_time ON public.slots USING btree (game_id, start_time);


--
-- TOC entry 4817 (class 1259 OID 25084)
-- Name: idx_slots_pending_expiry; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_slots_pending_expiry ON public.slots USING btree (status, pending_until) WHERE (status = 'Pending'::text);

CREATE INDEX idx_bookings_slot ON public.bookings USING btree (slot_id);
--
-- TOC entry 4831 (class 2606 OID 25072)
-- Name: bookings bookings_slot_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bookings
    ADD CONSTRAINT bookings_slot_id_fkey FOREIGN KEY (slot_id) REFERENCES public.slots(id) ON DELETE CASCADE;


--
-- TOC entry 4832 (class 2606 OID 25077)
-- Name: bookings bookings_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bookings
    ADD CONSTRAINT bookings_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- TOC entry 4830 (class 2606 OID 24986)
-- Name: game_settings game_settings_game_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.game_settings
    ADD CONSTRAINT game_settings_game_id_fkey FOREIGN KEY (game_id) REFERENCES public.games(id) ON DELETE CASCADE;


--
-- TOC entry 4827 (class 2606 OID 24938)
-- Name: games games_turf_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.games
    ADD CONSTRAINT games_turf_id_fkey FOREIGN KEY (turf_id) REFERENCES public.turfs(id) ON DELETE CASCADE;


--
-- TOC entry 4825 (class 2606 OID 16599)
-- Name: refresh_tokens refresh_tokens_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.refresh_tokens
    ADD CONSTRAINT refresh_tokens_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- TOC entry 4828 (class 2606 OID 24960)
-- Name: slots slots_game_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.slots
    ADD CONSTRAINT slots_game_id_fkey FOREIGN KEY (game_id) REFERENCES public.games(id) ON DELETE CASCADE;


--
-- TOC entry 4829 (class 2606 OID 24992)
-- Name: slots slots_settings_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.slots
    ADD CONSTRAINT slots_settings_id_fkey FOREIGN KEY (settings_id) REFERENCES public.game_settings(id);


--
-- TOC entry 4826 (class 2606 OID 16644)
-- Name: turfs turfs_manager_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.turfs
    ADD CONSTRAINT turfs_manager_id_fkey FOREIGN KEY (manager_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- TOC entry manually added: outbox
--

CREATE TABLE public.outbox (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    retry_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    process_after TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL
);

CREATE INDEX idx_outbox_queued ON public.outbox (created_at) 
WHERE status = 'queued' AND (process_after IS NULL OR process_after <= now());

--
-- TOC entry manually added: processed_jobs
--

CREATE TABLE public.processed_jobs (
    outbox_id UUID NOT NULL,
    consumer TEXT NOT NULL,
    processed_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
    PRIMARY KEY (outbox_id, consumer)
);

-- Completed on 2026-05-07 16:31:34

--
-- PostgreSQL database dump complete
--

\unrestrict qnNR1PpCqIoLa8KpmeIjEEMxLg2OHCBVVA7QlFRJsFQfoCavYCstWgtOgU5lqsX

